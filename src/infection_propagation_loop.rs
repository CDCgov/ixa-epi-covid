use core::f64;
use rand_distr::Binomial;

use crate::infectiousness_manager::{
    Forecast, InfectionContextExt, InfectionStatus, InfectionStatusValue, evaluate_forecast,
    get_forecast, infection_attempt,
};
use crate::parameters::{ContextParametersExt, Params};
use crate::rate_fns::{InfectiousnessRateExt, load_rate_fns};
use ixa::profiling::{increment_named_count, open_span};
use ixa::{
    Context, ContextPeopleExt, ContextRandomExt, IxaError, PersonId, PersonPropertyChangeEvent,
    define_rng, trace,
};

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

/// Takes susceptible people from the population and changes them according to a provided `seed_fn`.
/// The total number of people seeded is distributed binomially according to the proportion to seed.
/// The proportion to seed is calibrated to the population size, not the current number of susceptibles.
/// This may result in the entire susceptible population being seeded with `seed_fn`
#[allow(clippy::cast_possible_truncation)]
fn query_susceptibles_and_seed(
    context: &mut Context,
    proportion_to_seed: f64,
    seed_fn: impl Fn(&mut Context, PersonId),
) {
    let binom = Binomial::new(
        context.get_current_population().try_into().unwrap(),
        proportion_to_seed,
    )
    .unwrap();
    let k: u64 = context.sample_distr(InfectionRng, binom);
    trace!(
        "Altering {k} susceptibles with a seeding function using proportion {proportion_to_seed}."
    );

    if k > 0 {
        let susceptibles = context.sample_people(
            InfectionRng,
            (InfectionStatus, InfectionStatusValue::Susceptible),
            k as usize,
        );
        for person in susceptibles {
            seed_fn(context, person);
        }
    }
}

fn seed_initial_infections(context: &mut Context, initial_incidence: f64) {
    query_susceptibles_and_seed(context, initial_incidence, |context, person_id| {
        trace!("Infecting person {person_id} as an initial infection.");
        context.add_plan(0.0, move |context| {
            context.infect_person(person_id, None, None, None);
        });
    });
}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    let &Params {
        initial_incidence, ..
    } = context.get_params();

    load_rate_fns(context)?;
    seed_initial_infections(context, initial_incidence);

    // Subscribe to the person becoming infectious to trigger the infection propagation loop
    context.subscribe_to_event(
        |context, event: PersonPropertyChangeEvent<InfectionStatus>| {
            if event.current != InfectionStatusValue::Infectious {
                return;
            }
            schedule_next_forecasted_infection(context, event.person_id);
            schedule_recovery(context, event.person_id);
        },
    );

    Ok(())
}

#[cfg(test)]
mod test {
    use std::{cell::RefCell, rc::Rc};

    use ixa::{
        Context, ContextGlobalPropertiesExt, ContextPeopleExt, ContextRandomExt, ExecutionPhase,
        HashMap, IxaError, PersonId, PersonPropertyChangeEvent,
    };

    use ixa::assert_almost_eq;

    use crate::{
        define_setting_category,
        infection_propagation_loop::{
            InfectionStatus, InfectionStatusValue, init, schedule_next_forecasted_infection,
            schedule_recovery, seed_initial_infections,
        },
        infectiousness_manager::{
            InfectionContextExt, InfectionData, InfectionDataValue,
            max_total_infectiousness_multiplier,
        },
        parameters::{
            ContextParametersExt, CoreSettingsTypes, GlobalParams, ItinerarySpecificationType,
            Params, RateFnType,
        },
        rate_fns::{InfectiousnessRateExt, load_rate_fns},
        settings::{
            CensusTract, ContextSettingExt, Home, ItineraryEntry, SettingId, SettingProperties,
            Workplace,
        },
    };

    define_setting_category!(HomogeneousMixing);

    fn set_homogeneous_mixing_itinerary(
        context: &mut Context,
        person_id: PersonId,
    ) -> Result<(), IxaError> {
        let itinerary = vec![ItineraryEntry::new(
            SettingId::new(HomogeneousMixing, 0),
            1.0,
        )];
        context.add_itinerary(person_id, itinerary)
    }

    fn setup_context(seed: u64, rate: f64, alpha: f64, duration: f64) -> Context {
        let mut context = Context::new();
        let parameters = Params {
            seed,
            max_time: 100.0,
            initial_incidence: 0.1, // 10% of the population
            infectiousness_rate_fn: RateFnType::Constant { rate, duration },
            settings_properties: HashMap::from_iter(
                [
                    (
                        CoreSettingsTypes::Home,
                        SettingProperties {
                            alpha: 0.5,
                            itinerary_specification: Some(ItinerarySpecificationType::Constant {
                                ratio: 1.0,
                            }),
                        },
                    ),
                    (
                        CoreSettingsTypes::Workplace,
                        SettingProperties {
                            alpha: 0.5,
                            itinerary_specification: Some(ItinerarySpecificationType::Constant {
                                ratio: 1.0,
                            }),
                        },
                    ),
                    (
                        CoreSettingsTypes::CensusTract,
                        SettingProperties {
                            alpha: 0.5,
                            // Itinerary is specified in the `set_homogeneous_mixing_itinerary` function
                            // so we do not need to set it here.
                            itinerary_specification: None,
                        },
                    ),
                ]
                .into_iter()
                .collect::<HashMap<_, _>>(),
            ),
            ..Default::default()
        };
        context.init_random(parameters.seed);
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();

        // We also set up a homogenous mixing itinerary so that when we don't call `settings::init`,
        // we still have people in settings.
        context
            .register_setting_category(
                &HomogeneousMixing,
                SettingProperties {
                    alpha,
                    itinerary_specification: Some(ItinerarySpecificationType::Constant {
                        ratio: 1.0,
                    }),
                },
            )
            .unwrap();
        context
    }

    #[test]
    fn test_seed_initial_conditions() {
        let mut context = setup_context(0, 1.0, 1.0, 5.0);
        load_rate_fns(&mut context).unwrap();
        let initial_infected = context.add_person(()).unwrap();
        seed_initial_infections(&mut context, 1.0);
        // we check at time 0 to since individuals infections begin before time 0
        context.add_plan(0.0, move |context| {
            assert_eq!(
                context.get_person_property(initial_infected, InfectionStatus),
                InfectionStatusValue::Infectious
            );
        });
    }

    #[test]
    fn test_seed_initial_conditions_empty() {
        let mut context = setup_context(0, 1.0, 1.0, 5.0);
        load_rate_fns(&mut context).unwrap();
        let person = context.add_person(()).unwrap();
        seed_initial_infections(&mut context, 0.0);
        assert_eq!(
            context.get_person_property(person, InfectionStatus),
            InfectionStatusValue::Susceptible
        );
    }

    #[test]
    fn test_binomial_incidence() {
        let reps = 1000;
        let incidence = 0.5;
        let pop_size = 100;
        let num_initial_infections = Rc::new(RefCell::new(0));
        for rep in 0..reps {
            let num_initial_infections_clone: Rc<RefCell<usize>> =
                Rc::clone(&num_initial_infections);
            let mut context = setup_context(rep, 1.0, 1.0, 5.0);
            load_rate_fns(&mut context).unwrap();
            for _ in 0..pop_size {
                context.add_person(()).unwrap();
            }
            seed_initial_infections(&mut context, incidence);
            context.add_plan(0.0, move |context| {
                *num_initial_infections_clone.borrow_mut() +=
                    context.query_people_count((InfectionStatus, InfectionStatusValue::Infectious));
            });
            context.execute();
        }
        #[allow(clippy::cast_precision_loss, clippy::cast_lossless)]
        let observed_incidence =
            *num_initial_infections.borrow() as f64 / (reps as f64 * pop_size as f64);
        assert_almost_eq!(incidence, observed_incidence, 0.01);
    }

    #[test]
    fn test_init_loop() {
        let mut context = setup_context(42, 1.0, 1.0, 5.0);
        for _ in 0..10 {
            context.add_person(()).unwrap();
        }

        init(&mut context).unwrap();

        // At the end of 0.0, we should have some seeded infections and recovereds
        // based on the initial_infections parameter.
        context.add_plan_with_phase(
            0.0,
            move |context| {
                assert!(
                    !context
                        .query_people_count((InfectionStatus, InfectionStatusValue::Infectious))
                        == 0
                );
                assert!(
                    !context.query_people_count((InfectionStatus, InfectionStatusValue::Recovered))
                        == 0
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
            context.add_person(()).unwrap();
        }

        init(&mut context).unwrap();

        // We're going to extract out the number of initial infections and recovered
        let num_initial_infections = Rc::new(RefCell::new(0));
        let num_initial_infections_clone = Rc::clone(&num_initial_infections);

        context.add_plan(0.0, move |context| {
            // Count the number of initial infections and recovered actually created from the binomial
            // sampling
            *num_initial_infections_clone.borrow_mut() =
                context.query_people_count((InfectionStatus, InfectionStatusValue::Infectious));
        });

        // We want to count the number of new infections that are created to ensure this is equal to
        // the number of initial infections seeded.
        let num_new_infections = Rc::new(RefCell::new(0));
        let num_new_infections_clone = Rc::clone(&num_new_infections);

        context.subscribe_to_event(
            move |_context, event: PersonPropertyChangeEvent<InfectionStatus>| {
                if event.current == InfectionStatusValue::Infectious {
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

        // And that recovereds is equal to the initial infectious (who have recovered) + recovered
        assert_eq!(
            context.query_people_count((InfectionStatus, InfectionStatusValue::Recovered)),
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
            let p1 = context.add_person(()).unwrap();
            set_homogeneous_mixing_itinerary(&mut context, p1).unwrap();
            // We don't want infectious people beyond our index case to be able to transmit, so we
            // have to do setup on our own since just calling `init` will trigger a watcher for
            // people becoming infectious that lets them transmit.
            load_rate_fns(&mut context).unwrap();
            // Add our infectious fellow.
            let infectious_person = context.add_person(()).unwrap();
            set_homogeneous_mixing_itinerary(&mut context, infectious_person).unwrap();

            context.infect_person(infectious_person, None, None, None);
            // Get the total infectiousness multiplier for comparison to total number of infections.
            if total_infectiousness_multiplier.is_none() {
                total_infectiousness_multiplier = Some(max_total_infectiousness_multiplier(
                    &context,
                    infectious_person,
                ));
            }
            // Add a watcher for when people are infected to record the infection times.
            context.subscribe_to_event::<PersonPropertyChangeEvent<InfectionStatus>>(
                move |context, event| {
                    if event.current == InfectionStatusValue::Infectious {
                        let current_time = context.get_current_time();
                        infection_times_clone.borrow_mut().push(current_time);
                        // Reset the person to susceptible.
                        if event.person_id != infectious_person {
                            *num_infected_clone.borrow_mut() += 1;
                            context.set_person_property(
                                event.person_id,
                                InfectionData,
                                InfectionDataValue::Susceptible,
                            );
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
        println!(
            "Average number of infections over {num_sims} simulations: {avg_number_infections}"
        );
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
        let person = context.add_person(()).unwrap();
        context.infect_person(person, None, None, None);
        // For later, we need to get the recovery time from the rate function.
        context.execute();
        let recovery_time = context.get_person_rate_fn(person).infection_duration();
        schedule_recovery(&mut context, person);
        context.execute();
        // Make sure person is recovered.
        assert_eq!(
            context.get_person_property(person, InfectionData),
            InfectionDataValue::Recovered {
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
            [0.0, 0.0, 0.5],
            [0.0, 0.5, 0.0],
            [0.5, 0.0, 0.0],
            [0.5, 0.5, 0.0],
            [0.5, 0.0, 0.5],
            [0.0, 0.5, 0.5],
            [0.5, 0.5, 0.5],
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
                crate::settings::init(&mut context);

                // Add a a person who will get infected.
                let infectious_person = context.add_person(()).unwrap();
                let person_home = context.add_person(()).unwrap();
                let person_censustract = context.add_person(()).unwrap();
                let person_workplace = context.add_person(()).unwrap();
                let itinerary_all = vec![
                    ItineraryEntry::new(SettingId::new(Home, 0), ratio[0]),
                    ItineraryEntry::new(SettingId::new(CensusTract, 0), ratio[1]),
                    ItineraryEntry::new(SettingId::new(Workplace, 0), ratio[2]),
                ];
                let itinerary_home = vec![ItineraryEntry::new(SettingId::new(Home, 0), 1.0)];
                let itinerary_censustract =
                    vec![ItineraryEntry::new(SettingId::new(CensusTract, 0), 1.0)];
                let itinerary_workplace =
                    vec![ItineraryEntry::new(SettingId::new(Workplace, 0), 1.0)];
                context
                    .add_itinerary(infectious_person, itinerary_all)
                    .unwrap();
                context.add_itinerary(person_home, itinerary_home).unwrap();
                context
                    .add_itinerary(person_censustract, itinerary_censustract)
                    .unwrap();
                context
                    .add_itinerary(person_workplace, itinerary_workplace)
                    .unwrap();

                // We don't want infectious people beyond our index case to be able to transmit, so we
                // have to do setup on our own since just calling `init` will trigger a watcher for
                // people becoming infectious that lets them transmit.
                load_rate_fns(&mut context).unwrap();

                context.infect_person(infectious_person, None, None, None);
                // Get the total infectiousness multiplier for comparison to total number of infections.
                if total_infectiousness_multiplier.is_none() {
                    total_infectiousness_multiplier = Some(max_total_infectiousness_multiplier(
                        &context,
                        infectious_person,
                    ));
                }
                // Add a watcher for when people are infected to record their infection settings.
                context.subscribe_to_event::<PersonPropertyChangeEvent<InfectionStatus>>(
                    move |context, event| {
                        if event.current == InfectionStatusValue::Infectious {
                            // Reset the person to susceptible.
                            if event.person_id == person_home {
                                *num_infected_home_clone.borrow_mut() += 1;
                            } else if event.person_id == person_censustract {
                                *num_infected_cenustract_clone.borrow_mut() += 1;
                            } else if event.person_id == person_workplace {
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
        let mut initial_incidence = None;
        for seed in 0..num_sims {
            let mut context = setup_context(seed, 0.0, 0.0, 1.0);
            if initial_incidence.is_none() {
                // If we don't have an initial incidence, get it
                initial_incidence = Some(context.get_params().initial_incidence);
            }
            context.init_random(seed);
            // Add our people
            for _ in 0..num_people {
                context.add_person(()).unwrap();
            }
            init(&mut context).unwrap();
            // Add a plan to shutdown after the seeding so we can count infected and recovereds
            context.add_plan(0.0, ixa::Context::shutdown);
            context.execute();
            // Count number of initial infections and recovereds
            num_initial_infections +=
                context.query_people_count((InfectionStatus, InfectionStatusValue::Infectious));
        }
        // Check that the proportion of people is close to the expected proportion
        assert_almost_eq!(
            num_initial_infections as f64 / (num_people * num_sims) as f64,
            initial_incidence.unwrap(),
            0.01
        );
    }
}
