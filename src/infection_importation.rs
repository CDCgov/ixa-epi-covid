use core::f64;
use ixa::{csv, prelude::*};
use rand_distr::{Binomial, Uniform};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::ModelError;
use crate::infectiousness_manager::{InfectionContextExt, InfectionStatus};
use crate::parameters::{ContextParametersExt, Params};
use crate::population_loader::{Person, PersonId};
use crate::rate_fns::InfectiousnessRateExt;
use ixa::{Context, ContextRandomExt, define_rng, trace};

define_rng!(ImportationRng);

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ImportCasesFromFile {
    pub include: bool,
    pub filename: Option<PathBuf>,
}

#[derive(Deserialize, Debug)]
pub struct ImportationRecord {
    time: f64,
    imported_infections: usize,
}

trait NovelInfectionContextExt: PluginContext + ContextEntitiesExt + InfectiousnessRateExt {
    /// Seeds an infection for a given target person by sampling an infection time from a uniform distribution over the infectious period and then planning an infection for that person at the sampled time.
    /// The method samples the infection time in this way to ensure that the full infectious period is not elapsed by time t=0.0
    /// From the sample method reliance on `.get_person_rate_fn()`, the rate functions must already be loaded into the model when an infection is seeded.
    fn seed_infection(&mut self, target_id: PersonId) {
        // Sample offset for the infectious period.
        let uniform = Uniform::new(
            -self.get_person_rate_fn(target_id).infection_duration(),
            0.0,
        )
        .unwrap();

        let infection_time = self.sample_distr(ImportationRng, uniform);
        self.add_plan(infection_time, move |context| {
            context.infect_person(target_id, None, None, None);
        });
    }
}
impl NovelInfectionContextExt for Context {}

/// Takes susceptible people from the population and seeds them as infected according to a specified initial prevalence.
/// The total number of people seeded is distributed binomially according to the initial prevalence to seed.
/// The initial prevalence to seed is relative to the population size, not the current number of susceptibles.
/// This may result in the entire susceptible population being seeded as infected.
fn load_initial_prevalence(context: &mut Context) {
    let &Params {
        initial_prevalence, ..
    } = context.get_params();
    if initial_prevalence > 0.0 {
        let binom = Binomial::new(
            context.get_entity_count::<Person>() as u64,
            initial_prevalence,
        )
        .unwrap();
        let k: u64 = context.sample_distr(ImportationRng, binom);
        trace!(
            "Altering {k} susceptibles with a seeding function using proportion {initial_prevalence}."
        );
        let susceptibles = context.sample_entities::<Person, _, _>(
            ImportationRng,
            (InfectionStatus::Susceptible,),
            k as usize,
        );
        for susceptible in susceptibles {
            context.seed_infection(susceptible);
        }
    }
}

/// Receives a single line ImportationRecord struct and attempts to infect the specified imported infections count at time.
fn plan_importations(context: &mut Context, imported_infections: usize, time: f64) {
    context.add_plan(time, move |context| {
        let attempted_targets = context.sample_entities::<Person, _, _>(ImportationRng, (), imported_infections);

        for target_id in attempted_targets {
            if context.get_property::<Person, InfectionStatus>(target_id) == InfectionStatus::Susceptible {
                trace!("Importing infection for {target_id} at time {}.", time);
                context.infect_person(target_id, None, None, None);
            } else {
                trace!("Attempted to import infection for {target_id} at time {}, but they were not susceptible.", time);
            }
        }
    });
}

fn read_importation_schedule(
    context: &mut Context,
    importations_file: PathBuf,
) -> Result<(), ModelError> {
    let mut reader = csv::Reader::from_path(importations_file)?;
    let mut raw_record = csv::ByteRecord::new();
    let headers = reader.byte_headers()?.clone();

    while reader.read_byte_record(&mut raw_record)? {
        let record: ImportationRecord = raw_record.deserialize(Some(&headers))?;
        plan_importations(context, record.imported_infections, record.time);
    }
    Ok(())
}

fn load_importation_timeseries(context: &mut Context) -> Result<(), ModelError> {
    let Params {
        imported_cases_timeseries,
        ..
    } = context.get_params();
    if imported_cases_timeseries.include {
        if let Some(filename) = &imported_cases_timeseries.filename {
            read_importation_schedule(context, filename.clone())?;
        } else {
            return Err(ModelError::ModelError(
                "Importation from file is turned on but no filename was provided.".to_string(),
            ));
        }
    }
    Ok(())
}

/// Initializes the infection importation module by loading the initial prevalence and importation timeseries according to the provided parameters.
/// Infectiousness rate functions must be loaded in the model prior to initializng the initial prevalence because of dependency on infection duration
pub fn init(context: &mut Context) -> Result<(), ModelError> {
    load_initial_prevalence(context);
    load_importation_timeseries(context)?;
    Ok(())
}

#[cfg(test)]
mod test {
    use std::io::Write;
    use std::path::PathBuf;
    use std::{cell::RefCell, rc::Rc};
    use tempfile::NamedTempFile;

    use ixa::prelude::*;

    use ixa::assert_almost_eq;

    use crate::Age;
    use crate::error::ModelError;
    use crate::population_loader::PersonId;
    use crate::{
        infection_importation::{
            ImportCasesFromFile, init, load_importation_timeseries, load_initial_prevalence,
        },
        infectiousness_manager::InfectionStatus,
        parameters::{GlobalParams, Params},
        population_loader::Person,
        rate_fns::load_rate_fns,
    };

    fn persist_tmp_csv(content: &String) -> PathBuf {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        let (_file, path) = file.keep().unwrap();
        path
    }

    fn setup_context(
        seed: u64,
        initial_prevalence: f64,
        imported_infections_info: Option<ImportCasesFromFile>,
    ) -> Context {
        let mut context = Context::new();
        let imported_cases_timeseries = imported_infections_info.unwrap_or(ImportCasesFromFile {
            include: false,
            filename: None,
        });

        let parameters = Params {
            seed,
            max_time: 100.0,
            initial_prevalence,
            imported_cases_timeseries,
            ..Default::default()
        };
        context.init_random(parameters.seed);
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();
        load_rate_fns(&mut context).unwrap();
        context.set_start_time(-1000.);
        context
    }

    #[test]
    fn test_load_initial_prevalence() {
        let mut context = setup_context(0, 1.0, None);
        let initial_infected: PersonId = context.add_entity((Age(30),)).unwrap();
        load_initial_prevalence(&mut context);
        // we check at time 0 to since individuals infections begin before time 0
        context.add_plan(0.0, move |context| {
            assert_eq!(
                context.get_property::<Person, InfectionStatus>(initial_infected),
                InfectionStatus::Infectious
            );
        });
        context.execute();
    }

    #[test]
    fn test_load_initial_prevalence_empty() {
        let mut context = setup_context(0, 0.0, None);
        let person: PersonId = context.add_entity((Age(30),)).unwrap();
        load_initial_prevalence(&mut context);
        context.execute();
        assert_eq!(
            context.get_property::<Person, InfectionStatus>(person),
            InfectionStatus::Susceptible
        );
    }

    #[test]
    fn test_binomial_incidence() {
        let reps = 1000;
        let initial_prevalence = 0.5;
        let pop_size = 100;
        let num_initial_infections = Rc::new(RefCell::new(0));
        for rep in 0..reps {
            let num_initial_infections_clone: Rc<RefCell<usize>> =
                Rc::clone(&num_initial_infections);
            let mut context = setup_context(rep, initial_prevalence, None);
            for _ in 0..pop_size {
                context.add_entity::<Person, _>((Age(30),)).unwrap();
            }
            load_initial_prevalence(&mut context);
            context.add_plan(0.0, move |context| {
                *num_initial_infections_clone.borrow_mut() +=
                    context.query_entity_count::<Person, _>((InfectionStatus::Infectious,));
            });
            context.execute();
        }
        #[allow(clippy::cast_precision_loss, clippy::cast_lossless)]
        let observed_incidence =
            *num_initial_infections.borrow() as f64 / (reps as f64 * pop_size as f64);
        assert_almost_eq!(initial_prevalence, observed_incidence, 0.01);
    }

    #[test]
    fn test_too_many_importations() {
        // Import 150 infections in a completely susceptible population of 100 people.
        let input: String = String::from("time,imported_infections\n1.0,150\n");
        let synth_file = persist_tmp_csv(&input);
        let imported_infections_info = ImportCasesFromFile {
            include: true,
            filename: Some(synth_file),
        };
        let mut context = setup_context(0, 0.0, Some(imported_infections_info));

        for _ in 0..100 {
            context.add_entity::<Person, _>((Age(30),)).unwrap();
        }
        init(&mut context).unwrap();
        context.execute();

        let infecteds = context.query_entity_count::<Person, _>((InfectionStatus::Infectious,));
        let whole_population = context.get_entity_count::<Person>();
        assert_eq!(infecteds, whole_population);
    }

    #[test]
    fn test_no_importations_with_complete_initial_infection() {
        // Attempt to import infections into a population of 100 people that are all already infected.
        let input: String = String::from("time,imported_infections\n1.0,2\n");
        let synth_file = persist_tmp_csv(&input);
        let imported_infections_info = ImportCasesFromFile {
            include: true,
            filename: Some(synth_file),
        };
        let mut context = setup_context(0, 1.0, Some(imported_infections_info));

        for _ in 0..100 {
            context.add_entity::<Person, _>((Age(30),)).unwrap();
        }
        init(&mut context).unwrap();
        context.subscribe_to_event(move |context, event: PropertyChangeEvent<Person, InfectionStatus>| {
            if event.current == InfectionStatus::Infectious && context.get_current_time() > 0.0 {
                panic!("No infections should have been imported since the entire population was already infected from the initial prevalence seeding, but an infection was imported at time {}.", context.get_current_time());
            }
        });
        context.add_plan(0.0, move |context| {
            // Everyone should be infectious at time 0.0 from initial prevalence 1.0
            assert_eq!(
                context.query_entity_count::<Person, _>((InfectionStatus::Infectious,)),
                100
            );
        });
        context.execute();
    }

    #[test]
    fn test_zero_incidence_import_infections() {
        let input: String = String::from("time,imported_infections\n1.0,2\n3.0,3\n");
        let synth_file = persist_tmp_csv(&input);
        let imported_infections_info = ImportCasesFromFile {
            include: true,
            filename: Some(synth_file),
        };
        let mut context = setup_context(0, 0.0, Some(imported_infections_info));

        for _ in 0..1000 {
            context.add_entity::<Person, _>((Age(30),)).unwrap();
        }
        init(&mut context).unwrap();

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

        context.add_plan(1.0, move |context| {
            // At time 1.0, we should have 2 infections from the import file
            assert_eq!(
                context.query_entity_count::<Person, _>((InfectionStatus::Infectious,)),
                2
            );
        });

        context.add_plan(3.0, move |context| {
            // At time 3.0, we should have 3 additional infections from the import file (5 total)
            assert_eq!(
                context.query_entity_count::<Person, _>((InfectionStatus::Infectious,)),
                5
            );
        });

        context.execute();

        // Make sure that the only people who pass through infectious are those that we imported
        // as the initial infectious
        assert_eq!(*num_new_infections.borrow(), 5);
    }

    #[test]
    fn test_no_filename_include_importation() {
        let mut context = setup_context(
            0,
            0.0,
            Some(ImportCasesFromFile {
                include: true,
                filename: None,
            }),
        );

        let result = load_importation_timeseries(&mut context).err();
        match result {
            Some(ModelError::ModelError(message)) => {
                assert_eq!(
                    message,
                    "Importation from file is turned on but no filename was provided.".to_string()
                );
            }
            None => panic!("Expected a ModelError but got no error at all."),
            Some(_) => panic!(
                "Expected an ModelError with a specific message, but got a different error or no error at all."
            ),
        }
    }
}
