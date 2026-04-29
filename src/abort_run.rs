use crate::{
    parameters::{ContextParametersExt, Params},
    population_loader::Person,
    symptom_status_manager::SymptomStatus,
};
use ixa::prelude::*;

pub fn init(context: &mut Context) {
    let &Params {
        first_death_terminates_run,
        ..
    } = context.get_params();
    if first_death_terminates_run {
        context.subscribe_to_event::<PropertyChangeEvent<Person, SymptomStatus>>(
            move |context, event| {
                if event.current == SymptomStatus::Dead {
                    context.add_plan(context.get_current_time() + 1.0, move |context| {
                        context.shutdown();
                    });
                }
            },
        );
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::parameters::GlobalParams;
    use crate::{population_loader::Age, symptom_status_manager::SymptomData};
    use ixa::assert_almost_eq;

    fn setup(first_death_terminates_run: bool) -> Context {
        let mut context = Context::new();
        let parameters = Params {
            first_death_terminates_run,
            ..Default::default()
        };
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();
        init(&mut context);
        context
    }

    #[test]
    fn test_abort_run() {
        let mut context = setup(true);
        let person1 = context.add_entity::<Person, _>((Age(21),)).unwrap();
        let _ = context.add_entity::<Person, _>((Age(22),)).unwrap();
        context.add_plan(1.0, move |context| {
            context.set_property::<Person, SymptomData>(
                person1,
                SymptomData::Dead {
                    mild_time: 0.0,
                    severe_time: 0.0,
                    critical_time: 0.0,
                    dead_time: 1.0,
                },
            );
        });
        context.add_plan(3.0, move |context| {
            let _ = context.add_entity::<Person, _>((Age(23),)).unwrap();
        });
        context.execute();
        assert_eq!(context.get_entity_count::<Person>(), 2);
        assert_almost_eq!(context.get_current_time(), 2.0, 0.0);
    }

    #[test]
    fn test_continue_run() {
        let mut context = setup(false);
        let person1 = context.add_entity::<Person, _>((Age(21),)).unwrap();
        let _ = context.add_entity::<Person, _>((Age(22),)).unwrap();
        context.add_plan(1.0, move |context| {
            context.set_property::<Person, SymptomData>(
                person1,
                SymptomData::Dead {
                    mild_time: 0.0,
                    severe_time: 0.0,
                    critical_time: 0.0,
                    dead_time: 1.0,
                },
            );
        });
        context.add_plan(3.0, move |context| {
            let _ = context.add_entity::<Person, _>((Age(23),)).unwrap();
        });
        context.execute();
        assert_eq!(context.get_entity_count::<Person>(), 3);
        assert_almost_eq!(context.get_current_time(), 3.0, 0.0);
    }
}
