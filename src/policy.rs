use ixa::{entity::query::Query, prelude::*};
use serde::Serialize;

use crate::{
    ContextParametersExt,
    itinerary_manager::{ContextItineraryModifierExt, ItineraryModifier},
    settings::{ContextSettingExt, Person, SettingCode},
};

#[derive(Debug, PartialEq, Clone, Serialize, Copy)]
pub struct ItineraryModifierIntervention<T, U>
where
    T: Property<Person> + std::fmt::Debug,
    T::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
    U: Property<Person> + std::fmt::Debug,  
    U::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
{
    modifier: ItineraryModifier,
    active_property: T,
    inactive_property: U
}

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
pub enum Intervention<T, U>
where
    T: Property<Person> + std::fmt::Debug,
    T::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
    U: Property<Person> + std::fmt::Debug,
    U::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
{
    SchoolClosure(ItineraryModifierIntervention<T, U>),
    ShelterInPlace(ItineraryModifierIntervention<T, U>),
    Isolation(ItineraryModifierIntervention<T, U>),
}
//intervention trait with context and person,
// does this intervention apply
#[derive(Debug, PartialEq, Clone, Serialize, Copy)]
pub enum Group<Q>
where
    Q: Query<Person>,
{
    Population,
    GroupSetting(SettingCode),
    GroupQuery(Q),
}

#[derive(Debug, PartialEq, Clone, Serialize, Copy)]
pub struct Policy<T, U, Q>
where
    T: Property<Person> + std::fmt::Debug,
    T::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
    U: Property<Person> + std::fmt::Debug,
    U::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
    Q: Query<Person>,
{
    trigger: Trigger,
    intervention: Intervention<T, U>,
    group: Group<Q>,
}

#[derive(Debug, PartialEq, Clone, Serialize, Copy)]
pub struct IndividualLevelPolicy<P>
where
    P: Property<Person> + std::fmt::Debug,
    P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
{
    intervention: Intervention<P, P>
}

pub trait ContextPolicyExt:
    PluginContext
    + ContextEntitiesExt
    + ContextParametersExt
    + ContextItineraryModifierExt
    + ContextSettingExt
{
    fn add_policy<T, U, Q>(&mut self, policy: Policy<T, U, Q>)
    where
        T: Property<Person> + std::fmt::Debug,
        T::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
        U: Property<Person> + std::fmt::Debug,
        U::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
        Q: Query<Person>,
    {
        match policy.intervention {
            Intervention::SchoolClosure(_) | Intervention::ShelterInPlace(_) | Intervention::Isolation(_) => {
                self.add_policy_with_itinerary_modifier(policy);
            }
        }
    }

    fn add_policy_with_itinerary_modifier<T, U, Q>(&mut self, policy: Policy<T, U, Q>)
    where
        T: Property<Person> + std::fmt::Debug,
        T::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
        U: Property<Person> + std::fmt::Debug,
        U::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
        Q: Query<Person>,
    {
        let trigger = policy.trigger;
        let intervention = policy.intervention;
        let group = policy.group;
        let active_property = match &intervention {
            Intervention::SchoolClosure(modifier)
            | Intervention::ShelterInPlace(modifier)
            | Intervention::Isolation(modifier) => modifier.active_property,
        };
        let inactive_property = match &intervention {
            Intervention::SchoolClosure(modifier)
            | Intervention::ShelterInPlace(modifier)
            | Intervention::Isolation(modifier) => modifier.inactive_property,
        };

        // When should this list of people be collected. Currently it is on simulation initialization
        let person_ids = match group {
            Group::Population => self.get_entity_iterator::<Person>().collect(),
            Group::GroupSetting(setting_code) => self.get_setting_members(setting_code),
            Group::GroupQuery(query) => self.query_result_iterator::<Person, _>(query).collect(),
        };

        let person_ids_clone = person_ids.clone();

        match intervention {
            Intervention::SchoolClosure(modifier)
            | Intervention::ShelterInPlace(modifier)
            | Intervention::Isolation(modifier) => {
                self.register_itinerary_modifier(active_property, modifier.modifier);
            }
        }

        println!("person_ids: {:?}", person_ids);
        match trigger {
            Trigger::Time(time_trigger) => {
                self.add_plan(time_trigger.start_time, move |context| {
                    for person in &person_ids {
                        context.set_property::<Person, T>(*person, active_property);
                    }
                });
                self.add_plan(time_trigger.end_time, move |context| {
                    for person in &person_ids_clone {
                        context.set_property::<Person, U>(*person, inactive_property);
                    }
                });
            }
        }
    }

    fn add_individual_level_policy<P>(&mut self, policy: IndividualLevelPolicy<P>)
    where
        P: Property<Person> + std::fmt::Debug,
        P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
    {
        let intervention = policy.intervention;

        match intervention {
            Intervention::SchoolClosure(_)
            | Intervention::ShelterInPlace(_)
            | Intervention::Isolation(_) => {
                self.add_individual_level_itinerary_modifier_policy(policy);
            }
        }
    }

    fn add_individual_level_itinerary_modifier_policy<P>(&mut self, policy: IndividualLevelPolicy<P>)
    where
        P: Property<Person> + std::fmt::Debug,
        P::CanonicalValue: std::hash::Hash + Eq + std::fmt::Debug,
    {
        let intervention = policy.intervention;
        let active_property = match &intervention {
            Intervention::SchoolClosure(modifier)
            | Intervention::ShelterInPlace(modifier)
            | Intervention::Isolation(modifier) => modifier.active_property,
        };

        match intervention {
            Intervention::SchoolClosure(modifier)
            | Intervention::ShelterInPlace(modifier)
            | Intervention::Isolation(modifier) => {
                self.register_itinerary_modifier(active_property, modifier.modifier);
            }
        }
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
        settings::SettingCategory,
        symptom_status_manager::SymptomStatus,
    };

    use super::*;
    #[derive(Debug, PartialEq, Clone, Serialize, Copy, Eq, Hash)]
    pub struct DummyIsolating(pub bool);
    impl_property!(
        DummyIsolating,
        Person,
        default_const = DummyIsolating(false)
    );

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
        let isolation_intervention = ItineraryModifierIntervention {
            modifier: isolation_modifier,
            active_property: DummyIsolating(true),
            inactive_property: DummyIsolating(false),
        };
        let policy: Policy<DummyIsolating, DummyIsolating, ()> = Policy {
            trigger: Trigger::Time(TimeTrigger {
                start_time: 1.0,
                end_time: 5.0,
            }),
            intervention: Intervention::Isolation(isolation_intervention),
            group: Group::Population,
        };

        let p1 = context.add_entity(with!(Person, Age(30))).unwrap();
        let p2 = context.add_entity(with!(Person, Age(40))).unwrap();

        context.add_policy(policy);

        context.add_plan(0.5, move |context| {
            assert_eq!(
                context.get_property::<Person, DummyIsolating>(p1),
                DummyIsolating(false)
            );
            assert_eq!(
                context.get_property::<Person, DummyIsolating>(p2),
                DummyIsolating(false)
            );
        });

        context.add_plan(1.5, move |context| {
            assert_eq!(
                context.get_property::<Person, DummyIsolating>(p1),
                DummyIsolating(true)
            );
            assert_eq!(
                context.get_property::<Person, DummyIsolating>(p2),
                DummyIsolating(true)
            );
        });

        context.add_plan(5.5, move |context| {
            assert_eq!(
                context.get_property::<Person, DummyIsolating>(p1),
                DummyIsolating(false)
            );
            assert_eq!(
                context.get_property::<Person, DummyIsolating>(p2),
                DummyIsolating(false)
            );
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
        let isolation_intervention = ItineraryModifierIntervention {
            modifier: isolation_modifier,
            active_property: DummyIsolating(true),
            inactive_property: DummyIsolating(false),
        };
        let policy: Policy<DummyIsolating, DummyIsolating, (Age,)> = Policy {
            trigger: Trigger::Time(TimeTrigger {
                start_time: 1.0,
                end_time: 5.0,
            }),
            intervention: Intervention::Isolation(isolation_intervention),
            group: Group::GroupQuery((Age(30),)),
        };

        let p1 = context.add_entity(with!(Person, Age(30))).unwrap();
        let p2 = context.add_entity(with!(Person, Age(40))).unwrap();

        // Policies must be added after population is added
        context.add_policy(policy);

        context.add_plan(0.5, move |context| {
            assert_eq!(
                context.get_property::<Person, DummyIsolating>(p1),
                DummyIsolating(false)
            );
            assert_eq!(
                context.get_property::<Person, DummyIsolating>(p2),
                DummyIsolating(false)
            );
        });

        context.add_plan(1.5, move |context| {
            assert_eq!(
                context.get_property::<Person, DummyIsolating>(p1),
                DummyIsolating(true)
            );
            assert_eq!(
                context.get_property::<Person, DummyIsolating>(p2),
                DummyIsolating(false)
            );
        });

        context.add_plan(5.5, move |context| {
            assert_eq!(
                context.get_property::<Person, DummyIsolating>(p1),
                DummyIsolating(false)
            );
            assert_eq!(
                context.get_property::<Person, DummyIsolating>(p2),
                DummyIsolating(false)
            );
        });
        context.execute()
    }

    #[test]
    fn test_add_policy_setting_code() {
        let mut context = setup();

        let p1 = context.add_entity(with!(Person, Age(30))).unwrap();
        let p2 = context.add_entity(with!(Person, Age(40))).unwrap();

        let home_code_p1 = make_home_id(b"160379602000010");
        let home_code_p2 = make_home_id(b"160379602000020");
        context.add_person_to_settings(p1, Some(home_code_p1), None, None, None);
        context.add_person_to_settings(p2, Some(home_code_p2), None, None, None);

        let isolation_matrix = [
            [0.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
        ];

        let isolation_modifier = define_itinerary_modifier(Some(isolation_matrix), None);
        let isolation_intervention = ItineraryModifierIntervention {
            modifier: isolation_modifier,
            active_property: DummyIsolating(true),
            inactive_property: DummyIsolating(false),
        };
        let policy: Policy<DummyIsolating, DummyIsolating, ()> = Policy {
            trigger: Trigger::Time(TimeTrigger {
                start_time: 1.0,
                end_time: 5.0,
            }),
            intervention: Intervention::Isolation(isolation_intervention),
            group: Group::GroupSetting(home_code_p1),
        };

        context.add_policy(policy);

        context.add_plan(0.5, move |context| {
            assert_eq!(
                context.get_property::<Person, DummyIsolating>(p1),
                DummyIsolating(false)
            );
            assert_eq!(
                context.get_property::<Person, DummyIsolating>(p2),
                DummyIsolating(false)
            );
        });

        context.add_plan(1.5, move |context| {
            assert_eq!(
                context.get_property::<Person, DummyIsolating>(p1),
                DummyIsolating(true)
            );
            assert_eq!(
                context.get_property::<Person, DummyIsolating>(p2),
                DummyIsolating(false)
            );
        });

        context.add_plan(5.5, move |context| {
            assert_eq!(
                context.get_property::<Person, DummyIsolating>(p1),
                DummyIsolating(false)
            );
            assert_eq!(
                context.get_property::<Person, DummyIsolating>(p2),
                DummyIsolating(false)
            );
        });
        context.execute()
    }

    #[test]
    fn test_individual_level_policy() {
        let mut context = setup();

        let p1 = context.add_entity(with!(Person, Age(30))).unwrap();
        let p2 = context.add_entity(with!(Person, Age(40))).unwrap();

        let home_code_p1 = make_home_id(b"160379602000010");
        let home_code_p2 = make_home_id(b"160379602000020");
        let school_code_p1 = make_school_id(b"160379602000010");
        let school_code_p2 = make_school_id(b"160379602000020");
        let workplace_code_p1 = make_workplace_id(b"160379602000010");
        let workplace_code_p2 = make_workplace_id(b"160379602000020");
        let community_code_p1 = make_community_id(b"160379602000010");
        let community_code_p2 = make_community_id(b"160379602000020");
        context.add_person_to_settings(
            p1,
            Some(home_code_p1),
            Some(school_code_p1),
            Some(workplace_code_p1),
            Some(community_code_p1),
        );
        context.add_person_to_settings(
            p2,
            Some(home_code_p2),
            Some(school_code_p2),
            Some(workplace_code_p2),
            Some(community_code_p2),
        );

        let isolation_matrix = [
            [0.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0, 0.0],
        ];

        let isolation_modifier = define_itinerary_modifier(Some(isolation_matrix), None);
        let isolation_intervention = ItineraryModifierIntervention {
            modifier: isolation_modifier,
            active_property: SymptomStatus::Mild,
            inactive_property: SymptomStatus::Mild,
        };
        let individual_policy: IndividualLevelPolicy<SymptomStatus> = IndividualLevelPolicy {
            intervention: Intervention::Isolation(isolation_intervention),
        };

        context.add_individual_level_policy(individual_policy);

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
