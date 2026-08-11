use ixa::Context;
use serde::{Deserialize, Serialize};
use std::any::Any;

use crate::{
    itinerary_manager::ItineraryModifier,
    settings::{PersonId, SETTING_COUNT},
};

const TRANSIENT_STATE_COUNT: usize = SETTING_COUNT * 2;

pub type AcceptanceFunction = Box<dyn Fn(&Context, PersonId) -> bool>;

#[derive(Serialize, Deserialize)]
pub struct ItineraryTransitionMatrix {
    activity_matrix: [[f64; SETTING_COUNT]; SETTING_COUNT],
    location_matrix: [[f64; SETTING_COUNT]; SETTING_COUNT],
    #[serde(skip, default)]
    acceptance_function: Option<AcceptanceFunction>,
}

impl std::fmt::Debug for ItineraryTransitionMatrix {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ItineraryTransitionMatrix")
            .field("activity_matrix", &self.activity_matrix)
            .field("location_matrix", &self.location_matrix)
            .field(
                "has_acceptance_function",
                &self.acceptance_function.is_some(),
            )
            .finish()
    }
}

impl ItineraryModifier for ItineraryTransitionMatrix {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn layer(&self, other: &dyn ItineraryModifier) -> Box<dyn ItineraryModifier> {
        let other = other
            .as_any()
            .downcast_ref::<ItineraryTransitionMatrix>()
            .expect("incompatible modifier types for layering");

        Box::new(ItineraryTransitionMatrix::layer(self, other))
    }
    fn apply(&self, base_itinerary: &[f64; SETTING_COUNT]) -> [f64; SETTING_COUNT] {
        ItineraryTransitionMatrix::apply(self, base_itinerary)
    }
    fn accept(&self, context: &Context, person_id: PersonId) -> bool {
        self.acceptance_function
            .as_ref()
            .is_none_or(|acceptance| acceptance(context, person_id))
    }
}

#[allow(dead_code)]
#[allow(clippy::needless_range_loop)]
impl ItineraryTransitionMatrix {
    pub fn normalize(
        matrix: [[f64; SETTING_COUNT]; SETTING_COUNT],
    ) -> [[f64; SETTING_COUNT]; SETTING_COUNT] {
        let mut result = matrix;
        for i in 0..SETTING_COUNT {
            let row_sum: f64 = result[i].iter().sum();
            if row_sum > 1.0 {
                for j in 0..SETTING_COUNT {
                    result[i][j] /= row_sum;
                }
            }
        }
        result
    }

    pub fn build_transient_and_absorbing_matrix(
        &self,
    ) -> (
        [[f64; TRANSIENT_STATE_COUNT]; TRANSIENT_STATE_COUNT],
        [[f64; TRANSIENT_STATE_COUNT]; SETTING_COUNT],
    ) {
        let activity_matrix = ItineraryTransitionMatrix::normalize(self.activity_matrix);
        let location_matrix = ItineraryTransitionMatrix::normalize(self.location_matrix);
        let mut transient_matrix = [[0.0; TRANSIENT_STATE_COUNT]; TRANSIENT_STATE_COUNT];
        let mut absorbing_matrix = [[0.0; TRANSIENT_STATE_COUNT]; SETTING_COUNT];
        let activity_row_sums: Vec<f64> =
            activity_matrix.iter().map(|row| row.iter().sum()).collect();
        for i in 0..SETTING_COUNT {
            for j in 0..SETTING_COUNT {
                transient_matrix[i][j] = activity_matrix[i][j];
                if i != j {
                    transient_matrix[i + SETTING_COUNT][j + SETTING_COUNT] = location_matrix[i][j];
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

        // WARNING: You need to evaluated the acceptance function before they are layered.
        // If both modifiers have acceptance functions, the layered modifier will not have an acceptance function.
        ItineraryTransitionMatrix {
            activity_matrix: layered_activity_matrix,
            location_matrix: layered_location_matrix,
            acceptance_function: None,
        }
    }

    fn calculate_absorption_probabilities(&self) -> [[f64; SETTING_COUNT]; TRANSIENT_STATE_COUNT] {
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

        absorption_probs
    }

    pub fn apply(&self, current_itinerary: &[f64; SETTING_COUNT]) -> [f64; SETTING_COUNT] {
        let absorption_probs = self.calculate_absorption_probabilities();
        let mut new_itinerary = [0.0; SETTING_COUNT];
        for j in 0..SETTING_COUNT {
            for i in 0..SETTING_COUNT {
                new_itinerary[j] += current_itinerary[i] * absorption_probs[i][j];
            }
        }
        new_itinerary
    }
}

pub fn create_itinerary_transition_matrix(
    activity_matrix: Option<[[f64; SETTING_COUNT]; SETTING_COUNT]>,
    location_matrix: Option<[[f64; SETTING_COUNT]; SETTING_COUNT]>,
    acceptance_function: Option<AcceptanceFunction>,
) -> ItineraryTransitionMatrix {
    ItineraryTransitionMatrix {
        activity_matrix: activity_matrix.unwrap_or([[0.0; SETTING_COUNT]; SETTING_COUNT]),
        location_matrix: location_matrix.unwrap_or([[0.0; SETTING_COUNT]; SETTING_COUNT]),
        acceptance_function,
    }
}

pub fn assert_same_matrix(
    actual: &ItineraryTransitionMatrix,
    expected: &ItineraryTransitionMatrix,
) -> bool {
    if actual.activity_matrix != expected.activity_matrix {
        return false;
    }
    if actual.location_matrix != expected.location_matrix {
        return false;
    }
    if actual.acceptance_function.is_some() != expected.acceptance_function.is_some() {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_define_itinerary_modifier_with_none() {
        let modifier = create_itinerary_transition_matrix(None, None, None);
        assert_eq!(
            modifier.activity_matrix,
            [[0.0; SETTING_COUNT]; SETTING_COUNT]
        );
        assert_eq!(
            modifier.location_matrix,
            [[0.0; SETTING_COUNT]; SETTING_COUNT]
        );
    }

    #[test]
    fn test_define_itinerary_modifier_with_matrices() {
        let mut activity = [[0.0; SETTING_COUNT]; SETTING_COUNT];
        let mut location = [[0.0; SETTING_COUNT]; SETTING_COUNT];
        activity[0][0] = 0.5;
        location[0][1] = 0.3;

        let modifier = create_itinerary_transition_matrix(Some(activity), Some(location), None);
        assert_eq!(modifier.activity_matrix[0][0], 0.5);
        assert_eq!(modifier.location_matrix[0][1], 0.3);
    }

    #[test]
    fn test_normalize_matrix() {
        let matrix = ItineraryTransitionMatrix {
            activity_matrix: {
                let mut m = [[0.0; SETTING_COUNT]; SETTING_COUNT];
                m[0][0] = 0.4;
                m[0][1] = 0.6;
                m
            },
            location_matrix: [[0.0; SETTING_COUNT]; SETTING_COUNT],
            acceptance_function: None,
        };

        let matrix = ItineraryTransitionMatrix::normalize(matrix.activity_matrix);
        let sum: f64 = matrix[0].iter().sum();
        assert!(sum <= 1.0);
    }

    #[test]
    fn test_normalize_matrix_unnormalized() {
        let matrix = ItineraryTransitionMatrix {
            activity_matrix: {
                let mut m = [[0.0; SETTING_COUNT]; SETTING_COUNT];
                m[0][0] = 0.6;
                m[0][1] = 0.8;
                m
            },
            location_matrix: [[0.0; SETTING_COUNT]; SETTING_COUNT],
            acceptance_function: None,
        };

        let matrix = ItineraryTransitionMatrix::normalize(matrix.activity_matrix);
        let sum: f64 = matrix[0].iter().sum();
        assert!((sum - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_layer_matrices() {
        let matrix1 = ItineraryTransitionMatrix {
            activity_matrix: {
                let mut m = [[0.0; SETTING_COUNT]; SETTING_COUNT];
                m[0][0] = 0.3;
                m
            },
            location_matrix: [[0.0; SETTING_COUNT]; SETTING_COUNT],
            acceptance_function: None,
        };

        let matrix2 = ItineraryTransitionMatrix {
            activity_matrix: {
                let mut m = [[0.0; SETTING_COUNT]; SETTING_COUNT];
                m[0][0] = 0.2;
                m
            },
            location_matrix: [[0.0; SETTING_COUNT]; SETTING_COUNT],
            acceptance_function: None,
        };

        let layered = matrix1.layer(&matrix2);
        assert_eq!(layered.activity_matrix[0][0], 0.5);
    }

    #[test]
    fn test_build_transient_and_absorbing_matrix() {
        let matrix = ItineraryTransitionMatrix {
            activity_matrix: {
                let mut m = [[0.0; SETTING_COUNT]; SETTING_COUNT];
                m[0][0] = 0.5;
                m
            },
            location_matrix: [[0.0; SETTING_COUNT]; SETTING_COUNT],
            acceptance_function: None,
        };

        let (transient, absorbing) = matrix.build_transient_and_absorbing_matrix();
        assert_eq!(transient[0][0], 0.5);
        assert_eq!(transient[0][SETTING_COUNT], 0.5);
        assert_eq!(transient[1][SETTING_COUNT + 1], 1.0);
        assert_eq!(transient[2][SETTING_COUNT + 2], 1.0);
        assert_eq!(transient[3][SETTING_COUNT + 3], 1.0);
        assert_eq!(absorbing[0][SETTING_COUNT], 1.0);
        assert_eq!(absorbing[1][SETTING_COUNT + 1], 1.0);
        assert_eq!(absorbing[2][SETTING_COUNT + 2], 1.0);
        assert_eq!(absorbing[3][SETTING_COUNT + 3], 1.0);
    }
}
