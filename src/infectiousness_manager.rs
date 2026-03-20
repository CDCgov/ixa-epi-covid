use ixa::{impl_derived_property, prelude::*};
use rand_distr::Exp;
use serde::{Deserialize, Serialize};

use crate::{
    population_loader::PersonId,
    rate_fns::{InfectiousnessRateExt, InfectiousnessRateFn, ScaledRateFn},
    settings::{ContextSettingExt, WrappedSettingId},
};

use crate::population_loader::Person;

use ixa::profiling::{increment_named_count, open_span};

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy)]
pub enum InfectionData {
    Susceptible,
    Infectious {
        infection_time: f64,
        infected_by: Option<PersonId>,
        infection_setting_id: Option<WrappedSettingId>,
    },
    Recovered {
        infection_time: f64,
        recovery_time: f64,
    },
}

impl_property!(
    InfectionData,
    Person,
    default_const = InfectionData::Susceptible
);

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy, Hash, Eq)]
pub enum InfectionStatus {
    Susceptible,
    Infectious,
    Recovered,
}

impl_derived_property!(
    InfectionStatus,
    Person,
    [InfectionData],
    [],
    |infection_data| match infection_data {
        InfectionData::Susceptible => InfectionStatus::Susceptible,
        InfectionData::Infectious { .. } => InfectionStatus::Infectious,
        InfectionData::Recovered { .. } => InfectionStatus::Recovered,
    }
);

/// Calculate the scaling factor that accounts for the total infectiousness
/// for a person, given factors related to their environment, such as the number of people
/// they come in contact with or how close they are.
/// This is used to scale the intrinsic infectiousness function of that person.
/// There are no modifiers on intrinsic infectiousness
pub fn calc_total_infectiousness_multiplier(context: &Context, person_id: PersonId) -> f64 {
    context.calculate_current_infectiousness_multiplier_for_person(person_id)
}

/// Calculate the maximum possible scaling factor for total infectiousness
/// for a person, given information we know at the time of a forecast.
/// The modifier used for intrinsic infectiousness is ignored because all modifiers must
/// be less than or equal to one.
pub fn max_total_infectiousness_multiplier(context: &Context, person_id: PersonId) -> f64 {
    context.calculate_max_infectiousness_multiplier_for_person(person_id)
}

define_rng!(ForecastRng);

// Infection attempt function for a context and given `PersonId`
pub fn infection_attempt(context: &mut Context, person_id: PersonId) -> Option<PersonId> {
    let _span = open_span("infection_attempt");
    let setting = context.sample_active_setting(person_id).unwrap();
    if let Some(next_contact) = context
        .sample_from_setting_with_exclusion(person_id, setting)
        .unwrap() {
            match context.get_property::<Person, InfectionStatus>(next_contact) {
                InfectionStatus::Susceptible => {
                    increment_named_count("infection_success");
                    trace!(
                        "Infection attempt successful. Person {}, setting id {:?}, infecting {}",
                        person_id,
                        setting,
                        next_contact
                    );
                    context.infect_person(
                        next_contact,
                        Some(person_id),
                        Some(setting),
                    );
                    return Some(next_contact)
                }
                _ => return None,
            }
        }
    return None;
}

pub struct Forecast {
    pub next_time: f64,
    pub forecasted_total_infectiousness: f64,
}

/// Forecast of the next expected infection time, and the expected rate of
/// infection at that time.
pub fn get_forecast(context: &Context, person_id: PersonId) -> Option<Forecast> {
    // Get the person's individual infectiousness
    let rate_fn = context.get_person_rate_fn(person_id);
    // This scales infectiousness by the maximum possible infectiousness across all settings
    let scale = max_total_infectiousness_multiplier(context, person_id);
    let elapsed = context.get_elapsed_infection_time(person_id);
    let total_rate_fn = ScaledRateFn::new(rate_fn, scale, elapsed);

    // Draw an exponential and use that to determine the next time
    let exp = Exp::new(1.0).unwrap();
    let e = context.sample_distr(ForecastRng, exp);
    // Note: this returns None if forecasted > infectious period
    let t = total_rate_fn.inverse_cum_rate(e)?;

    let next_time = context.get_current_time() + t;
    let forecasted_total_infectiousness = total_rate_fn.rate(t);

    Some(Forecast {
        next_time,
        forecasted_total_infectiousness,
    })
}

/// Evaluates a forecast against the actual current infectious,
/// Returns a contact to be infected or None if the forecast is rejected
pub fn evaluate_forecast(
    context: &mut Context,
    person_id: PersonId,
    forecasted_total_infectiousness: f64,
) -> bool {
    let rate_fn = context.get_person_rate_fn(person_id);

    let total_multiplier = calc_total_infectiousness_multiplier(context, person_id);
    let total_rate_fn = ScaledRateFn::new(rate_fn, total_multiplier, 0.0);

    let elapsed_t = context.get_elapsed_infection_time(person_id);
    let current_infectiousness = total_rate_fn.rate(elapsed_t);

    assert!(
        // 1e-10 is a small enough tolerance for floating point comparison.
        current_infectiousness <= forecasted_total_infectiousness + 1e-10,
        "Person {person_id}: Forecasted infectiousness must always be greater than or equal to current infectiousness. Current: {current_infectiousness}, Forecasted: {forecasted_total_infectiousness}"
    );

    // If they are less infectious as we expected...
    if current_infectiousness < forecasted_total_infectiousness {
        // Reject with the ratio of current vs the forecasted
        if !context.sample_bool(
            ForecastRng,
            current_infectiousness / forecasted_total_infectiousness,
        ) {
            trace!("Person {person_id}: Forecast rejected");

            return false;
        }
    }

    true
}

pub trait InfectionContextExt: PluginContext + InfectiousnessRateExt {
    // This function should be called from the main loop whenever
    // someone is first infected. It assigns all their properties needed to
    // calculate intrinsic infectiousness

    // TODO: Should we check that target_id != source_id? 
    fn infect_person(
        &mut self,
        target_id: PersonId,
        source_id: Option<PersonId>,
        setting_id: Option<WrappedSettingId>,
    ) {
        let infection_time = self.get_current_time();
        trace!("Person {target_id}: Infected at {infection_time}");
        self.set_property::<Person, InfectionData>(
            target_id,
            InfectionData::Infectious {
                infection_time,
                infected_by: source_id,
                infection_setting_id: setting_id,
            },
        );
    }
    fn recover_person(&mut self, person_id: PersonId) {
        let recovery_time = self.get_current_time();
        let InfectionData::Infectious { infection_time, .. } =
            self.get_property::<Person, InfectionData>(person_id)
        else {
            panic!("Person {person_id} is not infectious")
        };
        self.set_property::<Person, InfectionData>(
            person_id,
            InfectionData::Recovered {
                recovery_time,
                infection_time,
            },
        );
    }
    fn get_elapsed_infection_time(&self, person_id: PersonId) -> f64 {
        let InfectionData::Infectious { infection_time, .. } =
            self.get_property::<Person, InfectionData>(person_id)
        else {
            panic!("Person {person_id} is not infectious")
        };
        self.get_current_time() - infection_time
    }
}
impl InfectionContextExt for Context {}

#[cfg(test)]
mod test {
    use super::{
        InfectionContextExt, evaluate_forecast, get_forecast, max_total_infectiousness_multiplier,
    };
    use crate::{
        Age, define_setting_category,
        error::ModelError,
        infectiousness_manager::{InfectionData, InfectionStatus},
        parameters::{GlobalParams, Params},
        population_loader::{Person, PersonId},
        rate_fns::{InfectiousnessRateExt, load_rate_fns},
        settings::{Alpha, HomeEntity, ItineraryEntry, SettingCode, SettingId, SettingProperties},
    };
    use ixa::{assert_almost_eq, prelude::*};

    define_setting_category!(HomogeneousMixing);

    fn set_homogeneous_mixing_itinerary(
        context: &mut Context,
        person_id: PersonId,
    ) -> Result<(), ModelError> {
        let itinerary = vec![ItineraryEntry::new(
            SettingId::new(HomogeneousMixing, 0),
            1.0,
        )];
        context.add_itinerary(person_id, itinerary)
    }

    fn setup_context() -> Context {
        let mut context = Context::new();
        context.init_random(0);
        context
            .set_global_property_value(
                GlobalParams,
                Params {
                    // For those tests that need infectious people, we add them manually.
                    max_time: 10.0,
                    ..Default::default()
                },
            )
            .unwrap();
        load_rate_fns(&mut context).unwrap();
        context
            .register_setting_category(&HomogeneousMixing, SettingProperties { alpha: 1.0 }, 1.0)
            .unwrap();

        context
    }

    #[test]
    fn test_infect_person() {
        let mut context = setup_context();
        let p1: PersonId = context.add_entity((Age(30),)).unwrap();
        context.add_plan(2.0, move |context| {
            context.infect_person(p1, None, None);
        });
        context.execute();
        let InfectionData::Infectious { infection_time, .. } =
            context.get_property::<Person, InfectionData>(p1)
        else {
            panic!("Person {p1} is not infectious")
        };
        assert_almost_eq!(infection_time, 2.0, 0.0);
        context.get_person_rate_fn(p1);
    }

    #[test]
    fn test_recover_person() {
        let mut context = setup_context();
        let p1: PersonId = context.add_entity((Age(30),)).unwrap();
        context.add_plan(2.0, move |context| {
            context.infect_person(p1, None, None);
        });
        context.add_plan(3.0, move |context| {
            context.recover_person(p1);
        });
        context.execute();
        let InfectionData::Recovered {
            infection_time,
            recovery_time,
        } = context.get_property::<Person, InfectionData>(p1)
        else {
            panic!("Person {p1} is not recovered")
        };
        assert_almost_eq!(infection_time, 2.0, 0.0);
        assert_almost_eq!(recovery_time, 3.0, 0.0);
    }

    #[test]
    fn test_get_elapsed_infection_time() {
        let mut context = setup_context();
        let p1: PersonId = context.add_entity((Age(30),)).unwrap();
        context.add_plan(2.0, move |context| {
            context.infect_person(p1, None, None);
        });
        // Run the simulation until time 3.0 at which the point the individual should have been
        // infected for 1.0 time units.
        context.add_plan(3.0, ixa::Context::shutdown);
        context.execute();
        // Check infection time
        assert_almost_eq!(context.get_elapsed_infection_time(p1), 1.0, 0.0);
    }

    #[test]
    fn test_calc_total_infectiousness_multiplier() {
        let mut context = setup_context();
        let p1: PersonId = context.add_entity((Age(30),)).unwrap();

        assert_almost_eq!(max_total_infectiousness_multiplier(&context, p1), 0.0, 0.0);
    }

    #[test]
    fn test_calc_total_infectiousness_multiplier_with_contact() {
        let mut context = setup_context();
        let p1: PersonId = context.add_entity((Age(30),)).unwrap();
        set_homogeneous_mixing_itinerary(&mut context, p1).unwrap();
        let p2: PersonId = context.add_entity((Age(30),)).unwrap();
        set_homogeneous_mixing_itinerary(&mut context, p2).unwrap();

        assert_almost_eq!(max_total_infectiousness_multiplier(&context, p1), 1.0, 0.0);
        assert_almost_eq!(max_total_infectiousness_multiplier(&context, p2), 1.0, 0.0);
    }

    #[test]
    /// Test has potential to stochastically fail if exponential draw is longer than infectious duration
    fn test_forecast() {
        let mut context = setup_context();
        let p1: PersonId = context.add_entity((Age(30),)).unwrap();
        set_homogeneous_mixing_itinerary(&mut context, p1).unwrap();
        // Add two additional contacts, which should make the factor 2
        let p2: PersonId = context.add_entity((Age(30),)).unwrap();
        set_homogeneous_mixing_itinerary(&mut context, p2).unwrap();
        let p3: PersonId = context.add_entity((Age(30),)).unwrap();
        set_homogeneous_mixing_itinerary(&mut context, p3).unwrap();

        context.infect_person(p1, None, None);

        let f = get_forecast(&context, p1).expect("Forecast should be returned");
        // The expected rate is 2.0, because intrinsic is 1.0 and there are 2 contacts.
        assert_almost_eq!(f.forecasted_total_infectiousness, 2.0, 0.0);
    }

    #[test]
    #[should_panic = "Person 0: Forecasted infectiousness must always be greater than or equal to current infectiousness. Current: 1, Forecasted: 0.9"]
    fn test_assert_evaluate_fails_when_forecast_smaller() {
        let mut context = setup_context();
        let p1: PersonId = context.add_entity((Age(30),)).unwrap();
        set_homogeneous_mixing_itinerary(&mut context, p1).unwrap();
        context.infect_person(p1, None, None);
        // We need to add another person so that our total infectiousness is 1.
        let p2: PersonId = context.add_entity((Age(30),)).unwrap();
        set_homogeneous_mixing_itinerary(&mut context, p2).unwrap();

        let invalid_forecast = 1.0 - 0.1;
        evaluate_forecast(&mut context, p1, invalid_forecast);
    }

    #[test]
    fn test_evaluate_still_succeeds_when_forecast_slightly_bigger() {
        let mut context = setup_context();
        let p1: PersonId = context.add_entity((Age(30),)).unwrap();
        set_homogeneous_mixing_itinerary(&mut context, p1).unwrap();
        context.infect_person(p1, None, None);
        let p2: PersonId = context.add_entity((Age(30),)).unwrap();
        set_homogeneous_mixing_itinerary(&mut context, p2).unwrap();

        let still_valid_forecast = 1.0 - 9e-11;
        assert!(evaluate_forecast(&mut context, p1, still_valid_forecast));
    }

    #[test]
    fn test_infected_options() {
        let mut context = setup_context();
        let index: PersonId = context.add_entity((Age(30),)).unwrap();
        let contact: PersonId = context.add_entity((Age(30),)).unwrap();
        let home_id = context.add_entity::<HomeEntity, _>((SettingCode(0), Alpha(0.1),)).unwrap();
        
        context.infect_person(contact, Some(index), Some(crate::settings::WrappedSettingId::Home(home_id)));
        context.execute();

        assert_eq!(
            context.get_property::<Person, InfectionStatus>(contact),
            InfectionStatus::Infectious
        );

        let InfectionData::Infectious {
            infected_by,
            infection_setting_type,
            infection_setting_id,
            ..
        } = context.get_property::<Person, InfectionData>(contact)
        else {
            panic!("Person {contact} is not infectious")
        };

        assert_eq!(infected_by.unwrap(), index);
        assert_eq!(infection_setting_type.unwrap(), "Home");
        assert_eq!(infection_setting_id.unwrap(), 0);
    }
}
