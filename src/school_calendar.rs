use crate::{
    ContextParametersExt, Params,
    error::ModelError,
    itinerary_manager::ContextItineraryModifierExt,
    itinerary_modifiers::{
        AcceptanceFunction, ItineraryTransitionMatrix, create_itinerary_transition_matrix,
    },
    settings::{Itinerary, Person, SettingCategory},
};
use ixa::{impl_derived_property, prelude::*};
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct Student(pub bool);

impl_derived_property!(Student, Person, [Itinerary], [], |itinerary| {
    Student(itinerary.setting_ids[SettingCategory::School].is_some())
});

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum SchoolCalendarModifierType {
    Weekend,
    SummerBreak,
    HolidayBreak,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SchoolCalendarModifier {
    pub modifier: SchoolCalendarModifierType,
    pub activates_at: f64,
    pub prop_school_time_to_home: f64,
    pub prop_school_time_to_comm: f64,
    pub deactivates_at: Option<f64>,
}

impl SchoolCalendarModifier {
    pub fn validate(&self) -> Result<(), ModelError> {
        if self.activates_at < 0.0 {
            return Err(ModelError::ModelError(
                "activates_at must be >= 0.0".to_string(),
            ));
        }
        if let Some(deactivates_at) = self.deactivates_at
            && deactivates_at < self.activates_at
        {
            return Err(ModelError::ModelError(
                "deactivates_at must be >= activates_at".to_string(),
            ));
        }
        if self.prop_school_time_to_home < 0.0 || self.prop_school_time_to_home > 1.0 {
            return Err(ModelError::ModelError(
                "prop_school_time_to_home must be in [0.0, 1.0]".to_string(),
            ));
        }
        if self.prop_school_time_to_comm < 0.0 || self.prop_school_time_to_comm > 1.0 {
            return Err(ModelError::ModelError(
                "prop_school_time_to_comm must be in [0.0, 1.0]".to_string(),
            ));
        }
        if self.prop_school_time_to_home + self.prop_school_time_to_comm > 1.0 {
            return Err(ModelError::ModelError(
                "prop_school_time_to_home + prop_school_time_to_comm must be <= 1.0".to_string(),
            ));
        }
        Ok(())
    }
}

impl PartialEq for SchoolCalendarModifier {
    fn eq(&self, other: &Self) -> bool {
        self.activates_at.to_bits() == other.activates_at.to_bits()
            && self.prop_school_time_to_home.to_bits() == other.prop_school_time_to_home.to_bits()
            && self.prop_school_time_to_comm.to_bits() == other.prop_school_time_to_comm.to_bits()
            && match (self.deactivates_at, other.deactivates_at) {
                (Some(a), Some(b)) => a.to_bits() == b.to_bits(),
                (None, None) => true,
                _ => false,
            }
            && self.modifier == other.modifier
    }
}

impl Eq for SchoolCalendarModifier {}

pub fn init(context: &mut Context) {
    let Params {
        school_calendar, ..
    } = context.get_params().clone();
    for modifier in school_calendar {
        let itinerary_modifier = define_school_calendar_itinerary_modifier(&modifier).unwrap();
        context.register_itinerary_modifier(Student(true), itinerary_modifier);
    }
}

fn define_school_calendar_itinerary_modifier(
    school_calendar_modifier: &SchoolCalendarModifier,
) -> Result<ItineraryTransitionMatrix, ModelError> {
    let params = school_calendar_modifier.clone();
    let matrix = [
        [0.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 0.0],
        [
            params.prop_school_time_to_home,
            0.0,
            0.0,
            params.prop_school_time_to_comm,
        ],
        [0.0, 0.0, 0.0, 0.0],
    ];

    match params.modifier {
        SchoolCalendarModifierType::Weekend => {
            let activates_at = params.activates_at;
            let acceptance: AcceptanceFunction = Box::new(move |context, _person_id| {
                context.get_current_time() % 7.0 >= activates_at
                    && context.get_current_time() % 7.0 < activates_at + 2.0
            });
            Ok(create_itinerary_transition_matrix(
                Some(matrix),
                None,
                Some(acceptance),
            ))
        }
        _ => {
            let activates_at = params.activates_at;
            let deactivates_at = params.deactivates_at;
            if let Some(deactivates_at) = deactivates_at {
                let acceptance: AcceptanceFunction = Box::new(move |context, _person_id| {
                    context.get_current_time() % 365.0 >= activates_at
                        && context.get_current_time() % 365.0 < deactivates_at
                });
                Ok(create_itinerary_transition_matrix(
                    Some(matrix),
                    None,
                    Some(acceptance),
                ))
            } else {
                Err(ModelError::ModelError(
                    "deactivates_at must be specified for non-weekend school calendar modifiers"
                        .to_string(),
                ))
            }
        }
    }
}

#[cfg(test)]
mod test {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::Age;
    use crate::parameters::{GlobalParams, Params, SettingProperties};
    use crate::pop_reader::parser::parse_fips_school_id;
    use crate::setting_code::SettingCode;
    use crate::settings::SettingCategory;
    use ixa::{ExecutionPhase, HashMap};

    fn make_school_id(school_id: &[u8]) -> SettingCode {
        SettingCode(parse_fips_school_id(school_id).unwrap().1)
    }

    fn setup(school_calendar_modifier: Vec<SchoolCalendarModifier>) -> Context {
        let mut context = Context::new();
        let parameters = Params {
            // We need to specify an itinerary split here even though we don't draw people from
            // itineraries because `load_synth_population` calls `create_itinerary` for each person,
            // and that function requires an itinerary write function to be set.
            settings_properties: HashMap::from_iter(
                [
                    (SettingCategory::Home, SettingProperties { alpha: 0.0 }),
                    (SettingCategory::School, SettingProperties { alpha: 0.0 }),
                    (SettingCategory::Work, SettingProperties { alpha: 0.0 }),
                    (SettingCategory::Community, SettingProperties { alpha: 0.0 }),
                ]
                .into_iter()
                .collect::<HashMap<_, _>>(),
            ),
            itinerary_ratios: HashMap::from_iter([
                (SettingCategory::Home, 0.25),
                (SettingCategory::School, 0.25),
                (SettingCategory::Work, 0.25),
                (SettingCategory::Community, 0.25),
            ]),
            school_calendar: school_calendar_modifier.clone(),
            ..Default::default()
        };
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();
        crate::settings::init(&mut context).unwrap();
        crate::school_calendar::init(&mut context);
        context
    }

    #[test]
    fn test_weekend_conditions() {
        let mut context = setup(vec![SchoolCalendarModifier {
            modifier: SchoolCalendarModifierType::Weekend,
            activates_at: 3.0,
            prop_school_time_to_home: 0.5,
            prop_school_time_to_comm: 0.5,
            deactivates_at: None,
        }]);
        let weekend = Rc::new(RefCell::new(0));
        let weekday = Rc::new(RefCell::new(0));
        let school_code = make_school_id(b"16037960200002");
        let p1 = context.add_entity(with!(Person, Age(10))).unwrap();
        context.set_property(
            p1,
            Itinerary {
                setting_ids: [None, None, Some(school_code), None],
                itinerary_ratios: [0.3, 0.0, 0.5, 0.2],
            },
        );
        for i in 0..20 {
            let weekend_clone: Rc<RefCell<usize>> = Rc::clone(&weekend);
            let weekday_clone: Rc<RefCell<usize>> = Rc::clone(&weekday);
            context.add_plan(i as f64, move |context| {
                let itinerary = context.get_itinerary(p1);
                if itinerary == [0.3, 0.0, 0.5, 0.2] {
                    *weekday_clone.borrow_mut() += 1;
                } else if itinerary == [0.55, 0.0, 0.0, 0.45] {
                    *weekend_clone.borrow_mut() += 1;
                }
            });
        }
        context.add_plan_with_phase(20.0, ixa::Context::shutdown, ExecutionPhase::Last);
        context.execute();
        #[allow(clippy::cast_precision_loss, clippy::cast_lossless)]
        let observed_weekend = *weekend.borrow();
        let observed_weekday = *weekday.borrow();
        // weekend starts on day 3, 10, and 17
        // weekend ends on day 4, 11, and 18
        assert_eq!(observed_weekend, 6);
        assert_eq!(observed_weekday, 14);
    }

    #[test]
    fn test_non_weekend_conditions() {
        let mut context = setup(vec![SchoolCalendarModifier {
            modifier: SchoolCalendarModifierType::SummerBreak,
            activates_at: 3.0,
            prop_school_time_to_home: 0.5,
            prop_school_time_to_comm: 0.5,
            deactivates_at: Some(5.0),
        }]);
        let summer_break = Rc::new(RefCell::new(0));
        let school_days = Rc::new(RefCell::new(0));
        let school_code = make_school_id(b"16037960200002");
        let p1 = context.add_entity(with!(Person, Age(10))).unwrap();
        context.set_property(
            p1,
            Itinerary {
                setting_ids: [None, None, Some(school_code), None],
                itinerary_ratios: [0.3, 0.0, 0.5, 0.2],
            },
        );
        for i in 0..20 {
            let summer_break_clone: Rc<RefCell<usize>> = Rc::clone(&summer_break);
            let school_days_clone: Rc<RefCell<usize>> = Rc::clone(&school_days);
            context.add_plan(i as f64, move |context| {
                let itinerary = context.get_itinerary(p1);
                if itinerary == [0.3, 0.0, 0.5, 0.2] {
                    *school_days_clone.borrow_mut() += 1;
                } else if itinerary == [0.55, 0.0, 0.0, 0.45] {
                    *summer_break_clone.borrow_mut() += 1;
                }
            });
        }
        context.add_plan_with_phase(20.0, ixa::Context::shutdown, ExecutionPhase::Last);
        context.execute();
        #[allow(clippy::cast_precision_loss, clippy::cast_lossless)]
        let observed_summer_break = *summer_break.borrow();
        let observed_school_days = *school_days.borrow();
        // summer break starts on day 3 and ends on day 4
        assert_eq!(observed_summer_break, 2);
        assert_eq!(observed_school_days, 18);
    }

    #[test]
    fn test_school_calendar_modifier_validation() {
        // Test valid modifier
        let valid_modifier = SchoolCalendarModifier {
            modifier: SchoolCalendarModifierType::Weekend,
            activates_at: 1.0,
            prop_school_time_to_home: 0.5,
            prop_school_time_to_comm: 0.3,
            deactivates_at: Some(2.0),
        };
        assert!(valid_modifier.validate().is_ok());

        // Test negative activates_at
        let neg_start = SchoolCalendarModifier {
            modifier: SchoolCalendarModifierType::Weekend,
            activates_at: -1.0,
            prop_school_time_to_home: 0.5,
            prop_school_time_to_comm: 0.3,
            deactivates_at: None,
        };
        assert!(neg_start.validate().is_err());

        // Test deactivates_at < activates_at
        let invalid_end = SchoolCalendarModifier {
            modifier: SchoolCalendarModifierType::SummerBreak,
            activates_at: 5.0,
            prop_school_time_to_home: 0.5,
            prop_school_time_to_comm: 0.3,
            deactivates_at: Some(2.0),
        };
        assert!(invalid_end.validate().is_err());

        // Test prop_school_time_to_home > 1.0
        let high_home = SchoolCalendarModifier {
            modifier: SchoolCalendarModifierType::HolidayBreak,
            activates_at: 1.0,
            prop_school_time_to_home: 1.5,
            prop_school_time_to_comm: 0.3,
            deactivates_at: None,
        };
        assert!(high_home.validate().is_err());

        // Test prop_school_time_to_comm < 0.0
        let neg_comm = SchoolCalendarModifier {
            modifier: SchoolCalendarModifierType::Weekend,
            activates_at: 1.0,
            prop_school_time_to_home: 0.5,
            prop_school_time_to_comm: -0.1,
            deactivates_at: None,
        };
        assert!(neg_comm.validate().is_err());

        // Test sum of proportions > 1.0
        let sum_too_high = SchoolCalendarModifier {
            modifier: SchoolCalendarModifierType::SummerBreak,
            activates_at: 1.0,
            prop_school_time_to_home: 0.7,
            prop_school_time_to_comm: 0.5,
            deactivates_at: None,
        };
        assert!(sum_too_high.validate().is_err());
    }
}
