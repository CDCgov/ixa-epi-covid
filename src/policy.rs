use ixa::{prelude::*, prelude_for_plugins::IxaEvent};
use serde::Serialize;

use crate::{
    ContextParametersExt,
    settings::{ContextSettingExt, Person},
};

pub trait TriggerEventTrait: IxaEvent + Copy {
    fn get_trigger_value(&self) -> f64;
}

#[derive(Debug, PartialEq, Clone, Serialize, Copy)]
pub struct Trigger<E: TriggerEventTrait, F: TriggerEventTrait> {
    pub start_event: E,
    pub end_event: Option<F>,
}

pub trait InterventionTrait: std::fmt::Debug + Copy + 'static {
    fn activate<P>(&self, context: &mut Context, group_property: P)
    where
        P: Property<Person> + std::fmt::Debug,
        P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug;
    fn deactivate<P>(&self, context: &mut Context, group_property: P)
    where
        P: Property<Person> + std::fmt::Debug,
        P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug;
}

#[derive(Debug, PartialEq, Clone, Serialize, Copy)]
pub struct Policy<P, I, E, F>
where
    P: Property<Person> + std::fmt::Debug,
    P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
    I: InterventionTrait,
    E: TriggerEventTrait,
    F: TriggerEventTrait,
{
    trigger: Trigger<E, F>,
    intervention: I,
    group: P,
}

pub trait ContextPolicyExt:
    PluginContext + ContextEntitiesExt + ContextParametersExt + ContextSettingExt
{
    fn add_policy<P, I, E, F>(&mut self, policy: Policy<P, I, E, F>)
    where
        P: Property<Person> + std::fmt::Debug,
        P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
        I: InterventionTrait,
        E: TriggerEventTrait + 'static,
        F: TriggerEventTrait + 'static,
    {
        let start_event = policy.trigger.start_event;
        let end_event = policy.trigger.end_event;
        self.subscribe_to_event::<E>(move |context, event| {
            let group_property = policy.group;
            let intervention = policy.intervention;
            if event.get_trigger_value() == start_event.get_trigger_value() {
                intervention.activate(context, group_property);
            } else if let Some(end_event) = end_event
                && event.get_trigger_value() == end_event.get_trigger_value()
            {
                intervention.deactivate(context, group_property);
            }
        });
    }
}
impl ContextPolicyExt for Context {}

#[cfg(test)]
mod tests {
    use ixa::{Context, HashMap};

    use crate::{
        Age, Params,
        custom_events::{ContextCustromEventExt, InfectionEvent, TimeEvent},
        infectiousness_manager::InfectionStatus,
        itinerary_manager::{ItineraryModifier, define_itinerary_modifier},
        parameters::{GlobalParams, SettingProperties},
        pop_reader::{
            FIPSCode, PopulationReaderSettingCategory,
            parser::{parse_fips_home_id, parse_fips_school_id, parse_fips_workplace_id},
        },
        population_loader::Alive,
        settings::{PersonId, SettingCategory, SettingCode},
        symptom_status_manager::SymptomStatus,
    };

    use super::*;
    fn make_home_id(home_id: &[u8]) -> SettingCode {
        SettingCode(parse_fips_home_id(home_id).unwrap().1)
    }

    fn make_school_id(school_id: &[u8]) -> SettingCode {
        SettingCode(parse_fips_school_id(school_id).unwrap().1)
    }

    fn make_workplace_id(workplace_id: &[u8]) -> SettingCode {
        SettingCode(parse_fips_workplace_id(workplace_id).unwrap().1)
    }

    fn make_community_id(home_id: &[u8]) -> SettingCode {
        let home_id = make_home_id(home_id).0;
        SettingCode(
            FIPSCode::with_category(
                home_id.state_code(),
                home_id.county_code(),
                home_id.census_tract_code(),
                PopulationReaderSettingCategory::CensusTract.encode(),
            )
            .unwrap(),
        )
    }

    fn add_person_to_test_settings(context: &mut Context, person: PersonId) {
        let home_code = make_home_id(b"160379602000010");
        let school_code = make_school_id(b"160379602000010");
        let workplace_code = make_workplace_id(b"160379602000010");
        let community_code = make_community_id(b"160379602000010");
        context.add_person_to_settings(
            person,
            Some(home_code),
            Some(school_code),
            Some(workplace_code),
            Some(community_code),
        );
    }

    fn setup() -> Context {
        let mut context = Context::new();
        let parameters = Params {
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
    fn test_add_policy_time_event() {
        let mut context = setup();

        let isolation_matrix = [
            [0.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
        ];

        let isolation_modifier = define_itinerary_modifier(Some(isolation_matrix), None);
        let policy: Policy<Alive, ItineraryModifier, TimeEvent, TimeEvent> = Policy {
            trigger: Trigger {
                start_event: TimeEvent { time: 1.0 },
                end_event: Some(TimeEvent { time: 5.0 }),
            },
            intervention: isolation_modifier,
            group: Alive(true),
        };

        let p1 = context.add_entity(with!(Person, Age(30))).unwrap();
        let p2 = context.add_entity(with!(Person, Age(40))).unwrap();

        add_person_to_test_settings(&mut context, p1);
        add_person_to_test_settings(&mut context, p2);

        context.add_policy(policy);

        context.add_plan(0.5, move |context| {
            assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 4);
            assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
        });

        context.add_plan(1.5, move |context| {
            assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 1);
            assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 1);
        });

        context.add_plan(5.5, move |context| {
            assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 4);
            assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
        });

        context.emit_time_event(1.0);
        context.emit_time_event(5.0);

        context.execute()
    }

    #[test]
    fn test_add_policy_infection_event() {
        let mut context = setup();

        context.subscribe_to_infections();

        let p1 = context.add_entity(with!(Person, Age(30))).unwrap();
        let p2 = context.add_entity(with!(Person, Age(40))).unwrap();

        add_person_to_test_settings(&mut context, p1);
        add_person_to_test_settings(&mut context, p2);

        let isolation_matrix = [
            [0.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
        ];

        let isolation_modifier = define_itinerary_modifier(Some(isolation_matrix), None);

        let policy: Policy<SymptomStatus, ItineraryModifier, InfectionEvent, InfectionEvent> =
            Policy {
                trigger: Trigger {
                    start_event: InfectionEvent { value: 1.0 },
                    end_event: None,
                },
                intervention: isolation_modifier,
                group: SymptomStatus::Mild,
            };

        context.add_policy(policy);

        context.add_plan(0.5, move |context| {
            assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 4);
            assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
        });

        context.add_plan(0.9, move |context| {
            context.set_property(p2, InfectionStatus::Infectious);
        });

        context.add_plan(1.0, move |context| {
            context.set_property(p1, SymptomStatus::Mild);
        });

        context.add_plan(1.5, move |context| {
            assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 1);
            assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
        });

        context.add_plan(2.0, move |context| {
            context.set_property(p1, SymptomStatus::Resolved);
        });

        context.add_plan(2.5, move |context| {
            assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 4);
            assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
        });
    }
}
