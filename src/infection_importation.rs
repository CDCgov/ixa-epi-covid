use core::f64;
use ixa::{csv, prelude::*};
use rand_distr::Binomial;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::infectiousness_manager::{InfectionContextExt, InfectionStatus};
use crate::parameters::{ContextParametersExt, Params};
use crate::population_loader::{Person, PersonId};
use ixa::{Context, ContextRandomExt, IxaError, define_rng, trace};

define_rng!(ImportationRng);

fn importation_attempt(context: &mut Context, target_id: PersonId) {
    context.infect_person(target_id, None, None, None);
}

/// Infects `infection_count` people at `infection_time` time. This is used for both seeding initial infections and importing infections from a file
fn plan_n_importations(context: &mut Context, infection_count: usize, infection_time: f64) {
    if infection_count > 0 {
        context.add_plan(infection_time, move |context| {
            let susceptibles = context.sample_entities::<Person, _, _>(
                ImportationRng,
                (InfectionStatus::Susceptible,),
                infection_count,
            );

            for person in susceptibles {
                trace!("Attempting to import infection for {person} at time {infection_time}.");
                importation_attempt(context, person);
            }
        });
    }
}

/// Takes susceptible people from the population and seeds them as infected.
/// The total number of people seeded is distributed binomially according to the initial incidence to seed.
/// The initial incidence to seed is relative to the population size, not the current number of susceptibles.
/// This may result in the entire susceptible population being seeded as infected
#[allow(clippy::cast_possible_truncation)]
fn seed_initial_infections(context: &mut Context, initial_incidence: f64) {
    let binom = Binomial::new(
        context.get_entity_count::<Person>() as u64,
        initial_incidence,
    )
    .unwrap();
    let k: u64 = context.sample_distr(ImportationRng, binom);
    trace!(
        "Altering {k} susceptibles with a seeding function using proportion {initial_incidence}."
    );

    plan_n_importations(context, k as usize, 0.0);
}

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

fn load_imported_infection_plan(
    context: &mut Context,
    importations_file: PathBuf,
) -> Result<(), IxaError> {
    let mut reader = csv::Reader::from_path(importations_file)?;
    let mut raw_record = csv::ByteRecord::new();
    let headers = reader.byte_headers()?.clone();

    while reader.read_byte_record(&mut raw_record)? {
        let record: ImportationRecord = raw_record.deserialize(Some(&headers))?;
        plan_n_importations(context, record.imported_infections, record.time);
    }
    Ok(())
}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    let Params {
        initial_incidence,
        import_cases_from_file,
        ..
    } = context.get_params();
    let initial_incidence = *initial_incidence;
    let import_cases_from_file = import_cases_from_file.clone();

    if initial_incidence > 0.0 {
        seed_initial_infections(context, initial_incidence);
    }
    if import_cases_from_file.include {
        if let Some(filename) = import_cases_from_file.filename {
            load_imported_infection_plan(context, filename)?;
        } else {
            return Err(IxaError::IxaError(
                "Importation from file is turned on but no filename was provided.".to_string(),
            ));
        }
    }

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
    use crate::population_loader::PersonId;
    use crate::{
        infection_importation::{ImportCasesFromFile, init, seed_initial_infections},
        infectiousness_manager::InfectionStatus,
        parameters::{GlobalParams, Params},
        population_loader::Person,
    };

    fn persist_tmp_csv(content: &String) -> PathBuf {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        let (_file, path) = file.keep().unwrap();
        path
    }

    fn setup_context(
        seed: u64,
        initial_incidence: f64,
        imported_infections_info: Option<ImportCasesFromFile>,
    ) -> Context {
        let mut context = Context::new();
        let import_cases_from_file = imported_infections_info.unwrap_or(ImportCasesFromFile {
            include: false,
            filename: None,
        });

        let parameters = Params {
            seed,
            max_time: 100.0,
            initial_incidence,
            import_cases_from_file,
            ..Default::default()
        };
        context.init_random(parameters.seed);
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();

        context
    }

    #[test]
    fn test_seed_initial_conditions() {
        let mut context = setup_context(0, 0.1, None);
        let initial_infected: PersonId = context.add_entity((Age(30),)).unwrap();
        seed_initial_infections(&mut context, 1.0);
        // we check at time 0 to since individuals infections begin before time 0
        context.add_plan(0.0, move |context| {
            assert_eq!(
                context.get_property::<Person, InfectionStatus>(initial_infected),
                InfectionStatus::Infectious
            );
        });
    }

    #[test]
    fn test_seed_initial_conditions_empty() {
        let mut context = setup_context(0, 0.1, None);
        let person: PersonId = context.add_entity((Age(30),)).unwrap();
        seed_initial_infections(&mut context, 0.0);
        assert_eq!(
            context.get_property::<Person, InfectionStatus>(person),
            InfectionStatus::Susceptible
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
            let mut context = setup_context(rep, 0.1, None);
            for _ in 0..pop_size {
                context.add_entity::<Person, _>((Age(30),)).unwrap();
            }
            seed_initial_infections(&mut context, incidence);
            context.add_plan(0.0, move |context| {
                *num_initial_infections_clone.borrow_mut() +=
                    context.query_entity_count::<Person, _>((InfectionStatus::Infectious,));
            });
            context.execute();
        }
        #[allow(clippy::cast_precision_loss, clippy::cast_lossless)]
        let observed_incidence =
            *num_initial_infections.borrow() as f64 / (reps as f64 * pop_size as f64);
        assert_almost_eq!(incidence, observed_incidence, 0.01);
    }

    #[test]
    fn test_too_many_importations() {
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

        let result = init(&mut context).err();
        match result {
            Some(IxaError::IxaError(message)) => {
                assert_eq!(
                    message,
                    "Importation from file is turned on but no filename was provided.".to_string()
                );
            }
            None => panic!("Expected an IxaError but got no error at all."),
            Some(_) => panic!(
                "Expected an IxaError with a specific message, but got a different error or no error at all."
            ),
        }
    }
}
