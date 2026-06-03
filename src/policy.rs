use ixa::prelude::*;
use serde::Serialize;

use crate::{
    ContextParametersExt,
    itinerary_manager::{ContextItineraryModifierExt, ItineraryModifier},
    settings::{ContextSettingExt, Person},
};

#[derive(Debug, PartialEq, Clone, Serialize, Copy)]
pub struct TimeTrigger {
    start_time: f64,
    end_time: f64,
}

#[derive(Debug, PartialEq, Clone, Serialize, Copy)]
pub enum Trigger {
    Time(TimeTrigger),
}

#[derive(Debug, PartialEq, Clone, Serialize, Copy)]
pub enum Intervention {
    SchoolClosure(ItineraryModifier),
    ShelterInPlace(ItineraryModifier),
    Isolation(ItineraryModifier),
}

#[derive(Debug, PartialEq, Clone, Serialize, Copy)]
pub struct ExogenousPolicy<P>
where
    P: Property<Person> + std::fmt::Debug,
    P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
{
    trigger: Trigger,
    intervention: Intervention,
    group: P,
}

#[derive(Debug, PartialEq, Clone, Serialize, Copy)]
pub struct EndogenousPolicy<P>
where
    P: Property<Person> + std::fmt::Debug,
    P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
{
    intervention: Intervention,
    group: P,
}

pub trait ContextPolicyExt:
    PluginContext
    + ContextEntitiesExt
    + ContextParametersExt
    + ContextItineraryModifierExt
    + ContextSettingExt
{
    fn add_exogenous_policy<P>(&mut self, policy: ExogenousPolicy<P>)
    where
        P: Property<Person> + std::fmt::Debug,
        P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
    {
        match policy.intervention {
            Intervention::SchoolClosure(_)
            | Intervention::ShelterInPlace(_)
            | Intervention::Isolation(_) => {
                self.add_exogenous_policy_with_itinerary_modifier(policy);
            }
        }
    }

    fn add_exogenous_policy_with_itinerary_modifier<P>(&mut self, policy: ExogenousPolicy<P>)
    where
        P: Property<Person> + std::fmt::Debug,
        P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
    {
        let trigger = policy.trigger;
        let intervention = policy.intervention;
        let group_property = policy.group;
        let intervention_modifier = match intervention {
            Intervention::SchoolClosure(modifier)
            | Intervention::ShelterInPlace(modifier)
            | Intervention::Isolation(modifier) => modifier,
        };
        match trigger {
            Trigger::Time(time_trigger) => {
                self.add_plan(time_trigger.start_time, move |context| {
                    context.register_itinerary_modifier::<P>(group_property, intervention_modifier);
                });
                self.add_plan(time_trigger.end_time, move |context| {
                    context.remove_itinerary_modifier_by_property::<P>(
                        group_property.make_canonical(),
                    );
                });
            }
        }
    }

    fn add_endogenous_policy<P>(&mut self, policy: EndogenousPolicy<P>)
    where
        P: Property<Person> + std::fmt::Debug,
        P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
    {
        let intervention = policy.intervention;

        match intervention {
            Intervention::SchoolClosure(_)
            | Intervention::ShelterInPlace(_)
            | Intervention::Isolation(_) => {
                self.add_endogenous_policy_with_itinerary_modifier(policy);
            }
        }
    }

    fn add_endogenous_policy_with_itinerary_modifier<P>(&mut self, policy: EndogenousPolicy<P>)
    where
        P: Property<Person> + std::fmt::Debug,
        P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
    {
        let modifer = match policy.intervention {
            Intervention::SchoolClosure(modifier)
            | Intervention::ShelterInPlace(modifier)
            | Intervention::Isolation(modifier) => modifier,
        };
        let group_property = policy.group;
        self.register_itinerary_modifier::<P>(group_property, modifer);
    }
}
impl ContextPolicyExt for Context {}

#[cfg(test)]
mod tests {
    use ixa::{Context, HashMap};

    use crate::{
        Age, Params,
        itinerary_manager::define_itinerary_modifier,
        parameters::{GlobalParams, SettingProperties},
        pop_reader::{
            FIPSCode, PopulationReaderSettingCategory,
            parser::{parse_fips_home_id, parse_fips_school_id, parse_fips_workplace_id},
        },
        population_loader::{Alive, HomeId},
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
    fn test_add_policy_population() {
        let mut context = setup();

        let isolation_matrix = [
            [0.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
        ];

        let isolation_modifier = define_itinerary_modifier(Some(isolation_matrix), None);
        let policy: ExogenousPolicy<Alive> = ExogenousPolicy {
            trigger: Trigger::Time(TimeTrigger {
                start_time: 1.0,
                end_time: 5.0,
            }),
            intervention: Intervention::Isolation(isolation_modifier),
            group: Alive(true),
        };

        let p1 = context.add_entity(with!(Person, Age(30))).unwrap();
        let p2 = context.add_entity(with!(Person, Age(40))).unwrap();

        add_person_to_test_settings(&mut context, p1);
        add_person_to_test_settings(&mut context, p2);

        context.add_exogenous_policy(policy);

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
        context.execute()
    }

    #[test]
    fn test_add_policy_query() {
        let mut context = setup();

        let isolation_matrix = [
            [0.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
        ];

        let isolation_modifier = define_itinerary_modifier(Some(isolation_matrix), None);

        let policy: ExogenousPolicy<Age> = ExogenousPolicy {
            trigger: Trigger::Time(TimeTrigger {
                start_time: 1.0,
                end_time: 5.0,
            }),
            intervention: Intervention::Isolation(isolation_modifier),
            group: Age(30),
        };

        let p1 = context.add_entity(with!(Person, Age(30))).unwrap();
        let p2 = context.add_entity(with!(Person, Age(40))).unwrap();

        add_person_to_test_settings(&mut context, p1);
        add_person_to_test_settings(&mut context, p2);

        // Policies must be added after population is added
        context.add_exogenous_policy(policy);

        context.add_plan(0.5, move |context| {
            assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 4);
            assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
        });

        context.add_plan(1.5, move |context| {
            assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 1);
            assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
        });

        context.add_plan(5.5, move |context| {
            assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 4);
            assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
        });
        context.execute()
    }

    #[test]
    fn test_add_policy_setting_code() {
        let mut context = setup();

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
        let policy: ExogenousPolicy<HomeId> = ExogenousPolicy {
            trigger: Trigger::Time(TimeTrigger {
                start_time: 1.0,
                end_time: 5.0,
            }),
            intervention: Intervention::Isolation(isolation_modifier),
            group: HomeId(Some(make_home_id(b"160379602000010"))),
        };

        context.add_exogenous_policy(policy);

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
        context.execute()
    }

    #[test]
    fn test_endogenous_policy() {
        let mut context = setup();

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

        let individual_policy: EndogenousPolicy<SymptomStatus> = EndogenousPolicy {
            intervention: Intervention::Isolation(isolation_modifier),
            group: SymptomStatus::Mild,
        };

        context.add_endogenous_policy(individual_policy);

        context.add_plan(0.5, move |context| {
            assert_eq!(context.get_active_settings_for_person(p1).unwrap().len(), 4);
            assert_eq!(context.get_active_settings_for_person(p2).unwrap().len(), 4);
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
