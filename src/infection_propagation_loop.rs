use ixa::prelude::*;

use crate::infectiousness_manager::{
    Forecast, InfectionContextExt, InfectionStatus, evaluate_forecast, get_forecast,
    infection_attempt,
};
use crate::population_loader::{Person, PersonId};
use crate::rate_fns::{InfectiousnessRateExt, load_rate_fns};
use ixa::profiling::{increment_named_count, open_span};
use ixa::{Context, IxaError, define_rng, trace};

define_rng!(InfectionRng);

fn schedule_next_forecasted_infection(context: &mut Context, person: PersonId) {
    if let Some(Forecast {
        next_time,
        forecasted_total_infectiousness,
    }) = get_forecast(context, person)
    {
        context.add_plan(next_time, move |context| {
            let _span = open_span("evaluate and schedule next forecast");
            if evaluate_forecast(context, person, forecasted_total_infectiousness) {
                let _ = infection_attempt(context, person);
            }
            // Continue scheduling forecasts until the person recovers.
            schedule_next_forecasted_infection(context, person);
        });
    }
}

fn schedule_recovery(context: &mut Context, person: PersonId) {
    let infection_duration = context.get_person_rate_fn(person).infection_duration();
    let recovery_time = context.get_current_time() + infection_duration;
    context.add_plan(recovery_time, move |context| {
        increment_named_count("recovery");
        trace!("Person {person} has recovered at {recovery_time}");
        context.recover_person(person);
    });
}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    load_rate_fns(context)?;
    // Subscribe to the person becoming infectious to trigger the infection propagation loop
    context.subscribe_to_event(
        |context, event: PropertyChangeEvent<Person, InfectionStatus>| {
            if event.current != InfectionStatus::Infectious {
                return;
            }

            schedule_recovery(context, event.entity_id);

            if context.get_current_time() > 0.0 {
                schedule_next_forecasted_infection(context, event.entity_id);
            } else {
                context.add_plan(0.0, move |context| {
                    schedule_next_forecasted_infection(context, event.entity_id);
                });
            }
        },
    );

    Ok(())
}

#[cfg(test)]
mod test {
    use std::{cell::RefCell, rc::Rc};

    use ixa::{ExecutionPhase, prelude::*};

    use ixa::{HashMap, assert_almost_eq};

    use crate::Age;
    use crate::infection_propagation_loop::InfectionRng;
    use crate::infectiousness_manager::InfectionData;
    use crate::population_loader::{CommunityId, HomeId, PersonId, WorkId};
    use crate::settings::{Alpha, CommunityEntity, HomeEntity, SettingCode, WorkEntity};
    use crate::{
        infection_propagation_loop::{
            InfectionStatus, init, schedule_next_forecasted_infection, schedule_recovery,
        },
        infectiousness_manager::{InfectionContextExt, max_total_infectiousness_multiplier},
        parameters::{ContextParametersExt, CoreSettingsTypes, GlobalParams, Params, RateFnType},
        population_loader::Person,
        rate_fns::{InfectiousnessRateExt, load_rate_fns},
        settings::SettingProperties,
    };

    fn set_homogeneous_mixing_itinerary(
        context: &mut Context,
        person_id: PersonId,
    ) -> Result<(), IxaError> {
        let community_id = context
            .query_result_iterator::<CommunityEntity, _>((SettingCode(0),))
            .next();
        if community_id.is_some() {
            context.set_property::<Person, CommunityId>(person_id, CommunityId(community_id));
        } else {
            context
                .add_entity::<CommunityEntity, _>((SettingCode(0), Alpha(0.0)))
                .map(|community_id| {
                    context.set_property::<Person, CommunityId>(
                        person_id,
                        CommunityId(Some(community_id)),
                    );
                })?;
        }
        Ok(())
    }

    fn setup_context(seed: u64, rate: f64, alpha: f64, duration: f64) -> Context {
        let mut context = Context::new();

        let parameters = Params {
            seed,
            max_time: 100.0,
            infectiousness_rate_fn: RateFnType::Constant { rate, duration },
            settings_properties: HashMap::from_iter(
                [
                    (CoreSettingsTypes::Home, SettingProperties { alpha }),
                    (CoreSettingsTypes::Workplace, SettingProperties { alpha }),
                    (
                        CoreSettingsTypes::CensusTract,
                        SettingProperties {
                            alpha,
                            // Itinerary is specified in the `set_homogeneous_mixing_itinerary` function
                            // so we do not need to set it here.
                        },
                    ),
                ]
                .into_iter()
                .collect::<HashMap<_, _>>(),
            ),
            itinerary_ratios: HashMap::from_iter([
                (CoreSettingsTypes::Home, 1.0),
                (CoreSettingsTypes::Workplace, 1.0),
                (CoreSettingsTypes::CensusTract, 1.0),
            ]),
            ..Default::default()
        };
        context.init_random(parameters.seed);
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();

        // We also set up a homogenous mixing itinerary so that when we don't call `settings::init`,
        // we still have people in settings.
        context
    }

    #[test]
    fn test_init_loop() {
        let mut context = setup_context(42, 1.0, 1.0, 5.0);
        for _ in 0..10 {
            context.add_entity::<Person, _>((Age(30),)).unwrap();
        }

        init(&mut context).unwrap();

        // At the end of 0.0, we should have some seeded infections and recovereds
        // based on the initial_infections parameter.
        context.add_plan_with_phase(
            0.0,
            move |context| {
                assert!(
                    !context.query_entity_count::<Person, _>((InfectionStatus::Infectious,)) == 0
                );
                assert!(
                    !context.query_entity_count::<Person, _>((InfectionStatus::Recovered,)) == 0
                );
            },
            ExecutionPhase::Last,
        );
    }

    #[test]
    fn test_zero_rate_no_infections() {
        let mut context = setup_context(0, 0.0, 1.0, 5.0);
        // Add people -- a lot so we can show that no new infections are added
        for _ in 0..1000 {
            context.add_entity::<Person, _>((Age(30),)).unwrap();
        }

        init(&mut context).unwrap();

        // We're going to extract out the number of initial infections and recovered
        let num_initial_infections = Rc::new(RefCell::new(100));
        let num_initial_infections_clone = Rc::clone(&num_initial_infections);

        context.add_plan(0.0, move |context| {
            let susceptibles = context.sample_entities::<Person, _, _>(
                InfectionRng,
                (InfectionStatus::Susceptible,),
                *num_initial_infections_clone.borrow(),
            );
            for person in susceptibles {
                context.infect_person(person, None, None);
            }
            // Count the number of initial infections and recovered actually created from the binomial
            // sampling
            *num_initial_infections_clone.borrow_mut() =
                context.query_entity_count::<Person, _>((InfectionStatus::Infectious,));
        });

        // We want to count the number of new infections that are created to ensure this is equal to
        // the number of initial infections seeded.
        let num_new_infections = Rc::new(RefCell::new(0));
        let num_new_infections_clone = Rc::clone(&num_new_infections);

        context.subscribe_to_event(
            move |_context, event: PropertyChangeEvent<Person, InfectionStatus>| {
                if event.current == InfectionStatus::Infectious {
                    *num_new_infections_clone.borrow_mut() += 1;
                }
            },
        );

        context.execute();

        // Make sure that the only people who pass through infectious are those that we seeded
        // as the initial infectious
        assert_eq!(
            *num_new_infections.borrow(),
            *num_initial_infections.borrow()
        );

        assert!(*num_initial_infections.borrow() > 0);

        // And that recovereds is equal to the initial infectious (who have recovered) + recovered
        assert_eq!(
            context.query_entity_count::<Person, _>((InfectionStatus::Recovered,)),
            *num_initial_infections.borrow(),
        );
    }

    #[test]
    fn test_number_timing_infections_one_time_unit() {
        // Does one infectious person generate the number of infections as expected by the rate?
        // We're going to run many simulations that each start with one infectious and one
        // susceptible person. The susceptible person gets moved back to susceptible when becoming
        // infected, so this is really a setup where there is no susceptible depletion/an
        // infinitely large starting population. We stop the simulation at the end of 1.0 time units
        // and compare the number of infected people to the infectious rate.
        // We're also going to check the times at which they are infected. In this test simulation,
        // we are using a constant hazard of infection, and we only record infection times that are
        // within 1.0 time units, so we expect the timing of infection attempts to follow U(0, 1).
        // First, we should not expect to observe an exponential distribution because we may observe
        // multiple infection attempts in the same experiment, not just the first. This also helps
        // provide intuition for why we expect a uniform distribution -- if the first infection
        // attempt happens quickly, that increases the chance we see another in 1.0 time units, and
        // because there is basically this compensating relationship between the time and the number
        // of events, they "cancel" each other out to give a uniform distribution (handwavingly).
        let num_sims: u64 = 20_000;
        let rate = 1.5;
        let alpha = 0.42;
        let duration = 5.0;
        // We need the total infectiousness multiplier for the person.
        let mut total_infectiousness_multiplier = None;
        // Where we store the infection times.
        let infection_times = Rc::new(RefCell::new(Vec::<f64>::new()));
        let num_infected = Rc::new(RefCell::new(0usize));
        for seed in 0..num_sims {
            let infection_times_clone = Rc::clone(&infection_times);
            let num_infected_clone = Rc::clone(&num_infected);
            let mut context = setup_context(seed, rate, alpha, duration);

            // We only run the simulation for 1.0 time units.
            context.add_plan_with_phase(1.0, ixa::Context::shutdown, ExecutionPhase::Last);
            // Add a a person who will get infected.
            let p1: PersonId = context.add_entity((Age(30),)).unwrap();
            set_homogeneous_mixing_itinerary(&mut context, p1).unwrap();
            // We don't want infectious people beyond our index case to be able to transmit, so we
            // have to do setup on our own since just calling `init` will trigger a watcher for
            // people becoming infectious that lets them transmit.
            load_rate_fns(&mut context).unwrap();
            // Add our infectious fellow.
            let infectious_person: PersonId = context.add_entity((Age(30),)).unwrap();
            set_homogeneous_mixing_itinerary(&mut context, infectious_person).unwrap();

            context.infect_person(infectious_person, None, None);
            // Get the total infectiousness multiplier for comparison to total number of infections.
            if total_infectiousness_multiplier.is_none() {
                total_infectiousness_multiplier = Some(max_total_infectiousness_multiplier(
                    &context,
                    infectious_person,
                ));
            }
            // Add a watcher for when people are infected to record the infection times.
            context.subscribe_to_event::<PropertyChangeEvent<Person, InfectionStatus>>(
                move |context, event| {
                    if event.current == InfectionStatus::Infectious {
                        let current_time = context.get_current_time();
                        infection_times_clone.borrow_mut().push(current_time);
                        // Reset the person to susceptible.
                        if event.entity_id != infectious_person {
                            *num_infected_clone.borrow_mut() += 1;
                            context.set_property(event.entity_id, InfectionData::Susceptible);
                        }
                    }
                },
            );
            // Setup is now over -- onto actually letting our infectious fellow infect others.
            schedule_next_forecasted_infection(&mut context, infectious_person);
            context.execute();
        }

        #[allow(clippy::cast_precision_loss)]
        let avg_number_infections = *num_infected.borrow() as f64 / num_sims as f64;
        assert_almost_eq!(
            avg_number_infections,
            rate * total_infectiousness_multiplier.unwrap(),
            0.05
        );
        // Check whether the times at when people are infected fall uniformly on [0, 1].
        check_ks_stat(&mut infection_times.borrow_mut(), |x| {
            // Manual CDF for Uniform[0, 1]: F(x) = 0 for x < a, F(x) = (x-a)/(b-a) for a ≤ x ≤ b, F(x) = 1 for x > b
            let a = 0.0;
            let b = 1.0;
            if x < a {
                0.0
            } else if x <= b {
                (x - a) / (b - a)
            } else {
                1.0
            }
        });
    }

    fn check_ks_stat(times: &mut [f64], theoretical_cdf: impl Fn(f64) -> f64) {
        // Sort the empirical times to make an empirical CDF.
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());

        // KS stat is the maximum observed CDF deviation.
        let ks_stat = times
            .iter()
            .enumerate()
            .map(|(i, time)| {
                #[allow(clippy::cast_precision_loss)]
                let empirical_cdf_value = (i as f64) / (times.len() as f64);
                let theoretical_cdf_value = theoretical_cdf(*time);
                (empirical_cdf_value - theoretical_cdf_value).abs()
            })
            .reduce(f64::max)
            .unwrap();

        assert_almost_eq!(ks_stat, 0.0, 0.01);
    }

    #[test]
    fn test_schedule_recovery() {
        // Create a simulation with an infected person and schedule their recovery.
        let mut context = setup_context(0, 0.0, 1.0, 5.0);
        load_rate_fns(&mut context).unwrap();
        let person: PersonId = context.add_entity((Age(30),)).unwrap();
        context.infect_person(person, None, None);
        // For later, we need to get the recovery time from the rate function.
        context.execute();
        let recovery_time = context.get_person_rate_fn(person).infection_duration();
        schedule_recovery(&mut context, person);
        context.execute();
        // Make sure person is recovered.
        assert_eq!(
            context.get_property::<Person, InfectionData>(person),
            InfectionData::Recovered {
                infection_time: 0.0,
                recovery_time
            }
        );
        // Make sure nothing has happened after person is recovered.
        assert_almost_eq!(context.get_current_time(), recovery_time, 0.0);
    }

    #[test]
    fn test_location_infections() {
        // Does one infectious person generate the number of infections as expected in different
        // settings? We're going to run many simulations that each start with one infectious and three
        // susceptible person. Each susceptible person belongs in one of three setting types
        // and the infectious person is in all three settings. The simulation ends after the
        // first person is infected. The location of this infection is records. We compare the number of
        // infected people in each setting to the expected proportion defined by the ratios. We examine
        // seven scenarios of ratios for the infectious individual.
        let num_sims: u64 = 1000;
        let rate = 1.5;
        let alpha = 0.42;

        // ratios is a matrix of ratio values for the three settings. The first value in each row
        // corresponds to the home setting, the second to the census tract setting, and the third to
        // the workplace setting.
        let ratios = [
            // [0.0, 0.0, 0.5],
            // [0.0, 0.5, 0.0],
            // [0.5, 0.0, 0.0],
            // [0.5, 0.5, 0.0],
            // [0.5, 0.0, 0.5],
            // [0.0, 0.5, 0.5],
            [1.0, 1.0, 1.0],
        ];
        for ratio in ratios {
            // We add home workplace and census tract settings to context
            // in the test setup for this unit test.
            // We need the total infectiousness multiplier for the person.
            let sum_of_ratio: f64 = ratio.iter().sum();
            let mut total_infectiousness_multiplier = None;
            // Where we store the infection counts.
            let num_infected_home = Rc::new(RefCell::new(0usize));
            let num_infected_censustract = Rc::new(RefCell::new(0usize));
            let num_infected_workplace = Rc::new(RefCell::new(0usize));

            for seed in 0..num_sims {
                let num_infected_home_clone = Rc::clone(&num_infected_home);
                let num_infected_cenustract_clone = Rc::clone(&num_infected_censustract);
                let num_infected_workplace_clone = Rc::clone(&num_infected_workplace);
                let mut context = setup_context(seed, rate, alpha, 5.0);

                // Add a a person who will get infected.
                let infectious_person: PersonId = context.add_entity((Age(30),)).unwrap();
                let person_home: PersonId = context.add_entity((Age(30),)).unwrap();
                let person_censustract: PersonId = context.add_entity((Age(30),)).unwrap();
                let person_workplace: PersonId = context.add_entity((Age(30),)).unwrap();

                let home_id = context
                    .add_entity::<HomeEntity, _>((SettingCode(0), Alpha(0.0)))
                    .unwrap();
                let workplace_id = context
                    .add_entity::<WorkEntity, _>((SettingCode(0), Alpha(0.0)))
                    .unwrap();
                let census_tract_id = context
                    .add_entity::<CommunityEntity, _>((SettingCode(0), Alpha(0.0)))
                    .unwrap();
                context.set_property::<Person, HomeId>(person_home, HomeId(Some(home_id)));
                context
                    .set_property::<Person, WorkId>(person_workplace, WorkId(Some(workplace_id)));
                context.set_property::<Person, CommunityId>(
                    person_censustract,
                    CommunityId(Some(census_tract_id)),
                );

                context.set_property::<Person, HomeId>(infectious_person, HomeId(Some(home_id)));
                context
                    .set_property::<Person, WorkId>(infectious_person, WorkId(Some(workplace_id)));
                context.set_property::<Person, CommunityId>(
                    infectious_person,
                    CommunityId(Some(census_tract_id)),
                );

                // We don't want infectious people beyond our index case to be able to transmit, so we
                // have to do setup on our own since just calling `init` will trigger a watcher for
                // people becoming infectious that lets them transmit.
                load_rate_fns(&mut context).unwrap();

                context.infect_person(infectious_person, None, None);
                // Get the total infectiousness multiplier for comparison to total number of infections.
                if total_infectiousness_multiplier.is_none() {
                    total_infectiousness_multiplier = Some(max_total_infectiousness_multiplier(
                        &context,
                        infectious_person,
                    ));
                }
                // Add a watcher for when people are infected to record their infection settings.
                context.subscribe_to_event::<PropertyChangeEvent<Person, InfectionStatus>>(
                    move |context, event| {
                        if event.current == InfectionStatus::Infectious {
                            // Reset the person to susceptible.
                            if event.entity_id == person_home {
                                *num_infected_home_clone.borrow_mut() += 1;
                            } else if event.entity_id == person_censustract {
                                *num_infected_cenustract_clone.borrow_mut() += 1;
                            } else if event.entity_id == person_workplace {
                                *num_infected_workplace_clone.borrow_mut() += 1;
                            }
                            context.shutdown();
                        }
                    },
                );
                // Setup is now over -- onto actually letting our infectious fellow infect others.
                schedule_next_forecasted_infection(&mut context, infectious_person);
                context.execute();
            }
            println!(
                "For ratio {:?}, average number of infections in home: {:?}, census tract: {:?}, workplace: {:?}",
                ratio,
                *num_infected_home.borrow() as f64 / num_sims as f64,
                *num_infected_censustract.borrow() as f64 / num_sims as f64,
                *num_infected_workplace.borrow() as f64 / num_sims as f64
            );

            #[allow(clippy::cast_precision_loss)]
            let avg_number_infections_home = *num_infected_home.borrow() as f64 / num_sims as f64;
            assert_almost_eq!(avg_number_infections_home, ratio[0] / sum_of_ratio, 0.05);
            #[allow(clippy::cast_precision_loss)]
            let avg_number_infections_censustract =
                *num_infected_censustract.borrow() as f64 / num_sims as f64;
            assert_almost_eq!(
                avg_number_infections_censustract,
                ratio[1] / sum_of_ratio,
                0.05
            );
            #[allow(clippy::cast_precision_loss)]
            let avg_number_infections_workplace =
                *num_infected_workplace.borrow() as f64 / num_sims as f64;
            assert_almost_eq!(
                avg_number_infections_workplace,
                ratio[2] / sum_of_ratio,
                0.05
            );
        }
    }

    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn test_proportion_infected_recovered() {
        // If we start with 1000 people 100 times, we should see that the proportion of people
        // who are initialized as infectious and recovered follow the expected proportions.
        let mut num_initial_infections = 0;
        let num_people = 1000;
        let num_sims = 10;
        let mut initial_prevalence = None;
        for seed in 0..num_sims {
            let mut context = setup_context(seed, 0.0, 0.0, 1.0);
            if initial_prevalence.is_none() {
                // If we don't have an initial incidence, get it
                initial_prevalence = Some(context.get_params().initial_prevalence);
            }
            context.init_random(seed);
            // Add our people
            for _ in 0..num_people {
                context.add_entity::<Person, _>((Age(30),)).unwrap();
            }
            init(&mut context).unwrap();
            // Add a plan to shutdown after the seeding so we can count infected and recovereds
            context.add_plan(0.0, ixa::Context::shutdown);
            context.execute();
            // Count number of initial infections and recovereds
            num_initial_infections +=
                context.query_entity_count::<Person, _>((InfectionStatus::Infectious,));
        }
        // Check that the proportion of people is close to the expected proportion
        assert_almost_eq!(
            num_initial_infections as f64 / (num_people * num_sims) as f64,
            initial_prevalence.unwrap(),
            0.01
        );
    }
}
