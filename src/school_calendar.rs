use ixa::{
    ExecutionPhase, IxaEvent,
    prelude::*,
    triggers::{ContextTriggersExt, PeriodicTimeTrigger, TriggerCriterion},
};

use crate::{
    ContextParametersExt, Params,
    itinerary_manager::ContextItineraryModifierExt,
    itinerary_modifiers::{ItineraryTransitionMatrix, define_itinerary_modifier},
    population_loader::Student,
};

#[derive(IxaEvent)]
struct Weekend {
    active: bool,
}

pub trait ContextCalendarExt:
    PluginContext + ContextEntitiesExt + ContextParametersExt + ContextTriggersExt
{
    fn setup_weekend_triggers(&mut self, delay: f64) {
        let weekend_start_trigger = PeriodicTimeTrigger::every(7.0)
            .with_phase(ExecutionPhase::Last)
            .start_at(delay)
            .emit_value(Weekend { active: true });
        let weekend_end_trigger = PeriodicTimeTrigger::every(7.0)
            .with_phase(ExecutionPhase::Last)
            .start_at(delay + 2.0)
            .emit_value(Weekend { active: false });
        self.register_trigger(weekend_start_trigger);
        self.register_trigger(weekend_end_trigger);
    }
    fn setup_weekend_itinerary_modification(
        &mut self,
        itinerary_modifier: ItineraryTransitionMatrix,
    ) {
        self.subscribe_to_event(move |context, event: Weekend| {
            if event.active {
                context.register_itinerary_modifier(Student(true), itinerary_modifier);
            } else {
                context.remove_itinerary_modifier_by_property(Student(true));
            }
        });
    }
}
impl ContextCalendarExt for Context {}

pub fn init(context: &mut Context) {
    let Params { weekends, .. } = context.get_params().clone();
    if let (Some(delay), Some(prop_school_time_to_home)) =
        (weekends.delay, weekends.prop_school_time_to_home)
    {
        context.setup_weekend_triggers(delay);
        let weekend_modifier = define_weekend_itinerary_modifier(prop_school_time_to_home);
        context.setup_weekend_itinerary_modification(weekend_modifier);
    }
}

fn define_weekend_itinerary_modifier(prop_school_time_to_home: f64) -> ItineraryTransitionMatrix {
    let weekend_matrix = [
        [0.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 0.0],
        [
            prop_school_time_to_home,
            0.0,
            0.0,
            1.0 - prop_school_time_to_home,
        ],
        [0.0, 0.0, 0.0, 0.0],
    ];

    define_itinerary_modifier(Some(weekend_matrix), None)
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
    use crate::settings::{Itinerary, Person, SettingCategory};
    use ixa::HashMap;

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
            ..Default::default()
        };
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();
        crate::settings::init(&mut context).unwrap();
        context
    }

    #[test]
    fn test_weekend_triggers() {
        let mut context = setup();
        let weekend_starts = Rc::new(RefCell::new(0));
        let weekend_starts_clone: Rc<RefCell<usize>> = Rc::clone(&weekend_starts);
        let weekend_ends = Rc::new(RefCell::new(0));
        let weekend_ends_clone: Rc<RefCell<usize>> = Rc::clone(&weekend_ends);
        context.setup_weekend_triggers(3.0);
        context.subscribe_to_event(move |cxt, event: Weekend| {
            if event.active {
                assert_eq!(cxt.get_current_time() % 7.0, 3.0);
                *weekend_starts_clone.borrow_mut() += 1;
            } else {
                assert_eq!(cxt.get_current_time() % 7.0, 5.0);
                *weekend_ends_clone.borrow_mut() += 1;
            }
        });
        context.add_plan_with_phase(18.0, ixa::Context::shutdown, ExecutionPhase::Last);
        context.execute();
        #[allow(clippy::cast_precision_loss, clippy::cast_lossless)]
        let observed_weekend_starts = *weekend_starts.borrow();
        let observed_weekend_ends = *weekend_ends.borrow();
        // weekend starts on day 3, 10, and 17
        // weekend ends on day 5, 12, and 19
        assert_eq!(observed_weekend_starts, 3);
        assert_eq!(observed_weekend_ends, 2);
    }

    #[test]
    fn test_itinerary_modification_registration() {
        let mut context = setup();
        let weekend_modifier = define_weekend_itinerary_modifier(0.5);
        let school_code = make_school_id(b"16037960200002");
        let p1 = context.add_entity(with!(Person, Age(10))).unwrap();
        context.setup_weekend_itinerary_modification(weekend_modifier);
        context.set_property(
            p1,
            Itinerary {
                setting_ids: [None, None, Some(school_code), None],
                itinerary_ratios: [0.3, 0.0, 0.5, 0.2],
            },
        );
        context.add_plan(1.0, move |context| {
            context.emit_event(Weekend { active: true });
        });
        context.add_plan(2.0, move |context| {
            context.emit_event(Weekend { active: false });
        });

        context.add_plan(0.0, move |context| {
            let itinerary = context.get_itinerary(p1);
            assert_eq!(itinerary, [0.3, 0.0, 0.5, 0.2]);
        });
        context.add_plan(1.5, move |context| {
            let itinerary = context.get_itinerary(p1);
            assert_eq!(itinerary, [0.55, 0.0, 0.0, 0.45]);
        });
        context.add_plan(3.0, move |context| {
            let itinerary = context.get_itinerary(p1);
            assert_eq!(itinerary, [0.3, 0.0, 0.5, 0.2]);
        });
        context.execute();
    }
}
