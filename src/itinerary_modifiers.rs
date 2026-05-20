use serde::Serialize;
use std::any::Any;

use crate::{itinerary_manager::ItineraryModifierTrait, settings::SETTING_COUNT};

const TRANSIENT_STATE_COUNT: usize = SETTING_COUNT * 2;

#[derive(Debug, PartialEq, Clone, Serialize, Copy)]
pub struct ItineraryTransitionMatrix {
    activity_matrix: [[f64; SETTING_COUNT]; SETTING_COUNT],
    location_matrix: [[f64; SETTING_COUNT]; SETTING_COUNT],
    absorption_probabilities: Option<[[f64; SETTING_COUNT]; TRANSIENT_STATE_COUNT]>,
}

impl ItineraryModifierTrait for ItineraryModifier {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn layer(&mut self, other: Box<dyn ItineraryModifierTrait>) -> Box<dyn ItineraryModifierTrait> {
        if let Some(other_modifier) = other.as_any().downcast_ref::<ItineraryModifier>() {
            Box::new(ItineraryModifier {
                modifier_activity: self
                    .modifier_activity
                    .layer(&other_modifier.modifier_activity),
            })
        } else {
            panic!("Incompatible modifier types for layering");
        }
    }

    fn apply(&mut self, base_itinerary: &[f64; SETTING_COUNT]) -> [f64; SETTING_COUNT] {
        self.modifier_activity.apply(base_itinerary)
    }
}

#[allow(dead_code)]
#[allow(clippy::needless_range_loop)]
impl ItineraryTransitionMatrix {
    pub fn normalize(&mut self) {
        let normalize_matrix = |matrix: &mut [[f64; SETTING_COUNT]; SETTING_COUNT]| {
            for i in 0..SETTING_COUNT {
                let row_sum: f64 = matrix[i].iter().sum();
                if row_sum > 1.0 {
                    for j in 0..SETTING_COUNT {
                        matrix[i][j] /= row_sum;
                    }
                }
            }
        };

        normalize_matrix(&mut self.activity_matrix);
        normalize_matrix(&mut self.location_matrix);
    }

    pub fn build_transient_and_absorbing_matrix(
        &self,
    ) -> (
        [[f64; TRANSIENT_STATE_COUNT]; TRANSIENT_STATE_COUNT],
        [[f64; TRANSIENT_STATE_COUNT]; SETTING_COUNT],
    ) {
        let mut transient_matrix = [[0.0; TRANSIENT_STATE_COUNT]; TRANSIENT_STATE_COUNT];
        let mut absorbing_matrix = [[0.0; TRANSIENT_STATE_COUNT]; SETTING_COUNT];
        let activity_row_sums: Vec<f64> = self
            .activity_matrix
            .iter()
            .map(|row| row.iter().sum())
            .collect();
        for i in 0..SETTING_COUNT {
            for j in 0..SETTING_COUNT {
                transient_matrix[i][j] = self.activity_matrix[i][j];
                if i != j {
                    transient_matrix[i + SETTING_COUNT][j + SETTING_COUNT] =
                        self.location_matrix[i][j];
                }
            }
            transient_matrix[i][i + SETTING_COUNT] = 1.0 - activity_row_sums[i];
        }

        let transient_row_sums: Vec<f64> = transient_matrix
            .iter()
            .map(|row| row.iter().sum())
            .collect();
        for i in 0..SETTING_COUNT {
            absorbing_matrix[i][i + SETTING_COUNT] = 1.0 - transient_row_sums[i + SETTING_COUNT];
        }
        (transient_matrix, absorbing_matrix)
    }

    pub fn layer(
        &self,
        itinerary_transition_matrix: &ItineraryTransitionMatrix,
    ) -> ItineraryTransitionMatrix {
        let mut layered_activity_matrix = [[0.0; SETTING_COUNT]; SETTING_COUNT];
        let mut layered_location_matrix = [[0.0; SETTING_COUNT]; SETTING_COUNT];

        for i in 0..SETTING_COUNT {
            for j in 0..SETTING_COUNT {
                layered_activity_matrix[i][j] =
                    self.activity_matrix[i][j] + itinerary_transition_matrix.activity_matrix[i][j];
                layered_location_matrix[i][j] =
                    self.location_matrix[i][j] + itinerary_transition_matrix.location_matrix[i][j];
            }
        }

        ItineraryTransitionMatrix {
            activity_matrix: layered_activity_matrix,
            location_matrix: layered_location_matrix,
            absorption_probabilities: None,
        }
    }

    fn calculate_absorption_probabilities(&mut self) {
        // Aborbing probabilities are calculated using the
        // formula N * R, where N is the fundamental matrix (I - Q)^-1, I is the identity matrix,
        // Q is the transient matrix, and R is the absorbing matrix.

        let (transient_matrix, absorbing_matrix) = self.build_transient_and_absorbing_matrix();

        // Create identity matrix I
        let mut identity = [[0.0; TRANSIENT_STATE_COUNT]; TRANSIENT_STATE_COUNT];
        for i in 0..TRANSIENT_STATE_COUNT {
            identity[i][i] = 1.0;
        }

        // Calculate I - Q (where Q is the transient matrix)
        let mut i_minus_q = [[0.0; TRANSIENT_STATE_COUNT]; TRANSIENT_STATE_COUNT];
        for i in 0..TRANSIENT_STATE_COUNT {
            for j in 0..TRANSIENT_STATE_COUNT {
                i_minus_q[i][j] = identity[i][j] - transient_matrix[i][j];
            }
        }

        // Invert (I - Q) to get the fundamental matrix N
        // Using Gaussian elimination with partial pivoting
        let n = i_minus_q;

        // Create augmented identity matrix for inversion
        let mut aug = [[0.0; 2 * TRANSIENT_STATE_COUNT]; TRANSIENT_STATE_COUNT];
        for i in 0..TRANSIENT_STATE_COUNT {
            for j in 0..TRANSIENT_STATE_COUNT {
                aug[i][j] = n[i][j];
                aug[i][j + TRANSIENT_STATE_COUNT] = if i == j { 1.0 } else { 0.0 };
            }
        }

        // Forward elimination with partial pivoting
        for col in 0..TRANSIENT_STATE_COUNT {
            // Find pivot
            let mut pivot_row = col;
            let mut max_val = aug[col][col].abs();
            for row in col + 1..TRANSIENT_STATE_COUNT {
                if aug[row][col].abs() > max_val {
                    max_val = aug[row][col].abs();
                    pivot_row = row;
                }
            }

            // Swap rows
            if pivot_row != col {
                aug.swap(col, pivot_row);
            }

            // Scale pivot row
            let pivot = aug[col][col];
            if pivot.abs() > f64::EPSILON {
                for j in 0..2 * TRANSIENT_STATE_COUNT {
                    aug[col][j] /= pivot;
                }

                // Eliminate column
                for row in 0..TRANSIENT_STATE_COUNT {
                    if row != col {
                        let factor = aug[row][col];
                        for j in 0..2 * TRANSIENT_STATE_COUNT {
                            aug[row][j] -= factor * aug[col][j];
                        }
                    }
                }
            }
        }

        // Extract inverted matrix and calculate absorption probabilities
        // N * R where N is the fundamental matrix (I-Q)^-1 and R is the absorbing matrix
        let mut absorption_probs = [[0.0; SETTING_COUNT]; TRANSIENT_STATE_COUNT];
        for i in 0..TRANSIENT_STATE_COUNT {
            for j in 0..SETTING_COUNT {
                for k in 0..TRANSIENT_STATE_COUNT {
                    absorption_probs[i][j] +=
                        aug[i][k + TRANSIENT_STATE_COUNT] * absorbing_matrix[j][k];
                }
            }
        }

        self.absorption_probabilities = Some(absorption_probs);
    }

    pub fn apply(&mut self, current_itinerary: &[f64; SETTING_COUNT]) -> [f64; SETTING_COUNT] {
        self.normalize();
        if self.absorption_probabilities.is_none() {
            self.calculate_absorption_probabilities();
        }
        let absorption_probs = self.absorption_probabilities.unwrap();
        let mut new_itinerary = [0.0; SETTING_COUNT];
        for j in 0..SETTING_COUNT {
            for i in 0..SETTING_COUNT {
                new_itinerary[j] += current_itinerary[i] * absorption_probs[i][j];
            }
        }
        new_itinerary
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Copy)]
pub struct ItineraryModifier {
    modifier_activity: ItineraryTransitionMatrix,
}

pub fn define_itinerary_modifier(
    activity_matrix: Option<[[f64; SETTING_COUNT]; SETTING_COUNT]>,
    location_matrix: Option<[[f64; SETTING_COUNT]; SETTING_COUNT]>,
) -> ItineraryModifier {
    let itinerary_transition_matrix = ItineraryTransitionMatrix {
        activity_matrix: activity_matrix.unwrap_or([[0.0; SETTING_COUNT]; SETTING_COUNT]),
        location_matrix: location_matrix.unwrap_or([[0.0; SETTING_COUNT]; SETTING_COUNT]),
        absorption_probabilities: None,
    };
    ItineraryModifier {
        modifier_activity: itinerary_transition_matrix,
    }
}
