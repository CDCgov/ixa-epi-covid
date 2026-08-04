use ixa::{impl_derived_property, prelude::*};
use serde::Serialize;
use std::sync::Arc;

use crate::{
    ContextParametersExt, Params,
    itinerary_manager::ContextItineraryModifierExt,
    itinerary_modifiers::{
        AcceptanceFunction, ItineraryTransitionMatrix, define_itinerary_modifier,
    },
    settings::{Itinerary, Person, SettingCategory},
};

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct Student(pub bool);

impl_derived_property!(Student, Person, [Itinerary], [], |itinerary| {
    Student(itinerary.setting_ids[SettingCategory::School].is_some())
});

pub fn init(context: &mut Context) {
    let Params { weekends, .. } = context.get_params().clone();
    if let (Some(delay), Some(prop_school_time_to_home), Some(prop_school_time_to_comm)) = (
        weekends.delay,
        weekends.prop_school_time_to_home,
        weekends.prop_school_time_to_comm,
    ) {
        let weekend_modifier = define_weekend_itinerary_modifier(
            prop_school_time_to_home,
            prop_school_time_to_comm,
            delay,
        );
        context.register_itinerary_modifier(Student(true), weekend_modifier);
    }
}

fn define_weekend_itinerary_modifier(
    prop_school_time_to_home: f64,
    prop_school_time_to_comm: f64,
    delay: f64,
) -> ItineraryTransitionMatrix {
    let weekend_matrix = [
        [0.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 0.0],
        [prop_school_time_to_home, 0.0, 0.0, prop_school_time_to_comm],
        [0.0, 0.0, 0.0, 0.0],
    ];
    let acceptance: AcceptanceFunction = Arc::new(move |context, _person| {
        context.get_current_time() % 7.0 >= delay && context.get_current_time() % 7.0 <= delay + 2.0
    });
    define_itinerary_modifier(Some(weekend_matrix), None, Some(acceptance))
}

#[cfg(test)]
mod test {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::Age;
    use crate::parameters::{GlobalParams, Params, SettingProperties, Weekends};
    use crate::pop_reader::parser::parse_fips_school_id;
    use crate::setting_code::SettingCode;
    use crate::settings::SettingCategory;
    use ixa::{ExecutionPhase, HashMap};

    fn make_school_id(school_id: &[u8]) -> SettingCode {
        SettingCode(parse_fips_school_id(school_id).unwrap().1)
    }

    fn setup() -> Context {
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
            weekends: Weekends {
                delay: Some(3.0),
                prop_school_time_to_home: Some(0.5),
                prop_school_time_to_comm: Some(0.5),
            },
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
        let mut context = setup();
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
        // weekend ends on day 5, 12, and 19
        assert_eq!(observed_weekend, 9);
        assert_eq!(observed_weekday, 11);
    }
}
