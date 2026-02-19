use ixa::{csv, prelude::*, HashMap};

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::{
    parameters::{ContextParametersExt, Params},
    setting_loader::{CensusTract, CensusTractId, DefaultSettingCategory, GeographyProperties, Home, HomeId, School, SchoolId, Workplace, WorkplaceId},
};

use ixa::profiling::open_span;

define_entity!(Person);

#[derive(Deserialize, Debug)]
#[allow(non_snake_case)]
pub struct PeopleRecord<'a> {
    age: u8,
    homeId: &'a [u8],
    schoolId: &'a [u8],
    workplaceId: &'a [u8],
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct Age(pub u8);
impl_property!(Age, Person);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct Alive(pub bool);
impl_property!(Alive, Person, default_const = Alive(true));

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct PersonHomeId(pub HomeId);
impl_property!(PersonHomeId, Person);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct PersonSchoolId(pub Option<SchoolId>);
impl_property!(PersonSchoolId, Person);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct PersonWorkplaceId(pub Option<WorkplaceId>);
impl_property!(PersonWorkplaceId, Person);
#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct PersonCensusTractId(pub CensusTractId);
impl_property!(PersonCensusTractId, Person);

#[derive(Debug, PartialEq, Clone, Copy, Serialize)]
pub struct ItineraryEntry {
    pub home_ratio: Option<f64>,
    pub school_ratio: Option<f64>,
    pub workplace_ratio: Option<f64>,
    pub census_tract_ratio: Option<f64>,
}
impl_property!(ItineraryEntry, Person, default_const = ItineraryEntry {
    home_ratio: None,
    school_ratio: None,
    workplace_ratio: None,
    census_tract_ratio: None,
});

fn create_person_from_record(
    context: &mut Context,
    person_record: &PeopleRecord,
    itinerary_ratios: HashMap<DefaultSettingCategory, f64>,
) -> Result<(), IxaError> {
    // Create itinerary entries for all setting memberships in input file
    let tract: String = String::from_utf8(person_record.homeId[..11].to_owned())?;
    let home_id: String = String::from_utf8(person_record.homeId.to_owned())?;
    let school_string: String = String::from_utf8(person_record.schoolId.to_owned())?;
    let workplace_string: String = String::from_utf8(person_record.workplaceId.to_owned())?;

    let home_setting_id: HomeId = context.query_result_iterator::<Home,_>((
        GeographyProperties {
            fips_code: home_id.parse().map_err(|_| IxaError::IxaError(format!("Invalid FIPS code: {}", home_id)))?,
        },
    )).next().ok_or_else(|| IxaError::IxaError(format!("No setting found with string: {}", home_id)))?;

    let tract_setting_id: CensusTractId = context.query_result_iterator::<CensusTract,_>((
        GeographyProperties {
            fips_code: tract.parse().map_err(|_| IxaError::IxaError(format!("Invalid FIPS code: {}", tract)))?,
        },
    )).next().ok_or_else(|| IxaError::IxaError(format!("No setting found with string: {}", tract)))?;

    let school_setting_id: Option<SchoolId> = if school_string.is_empty() {
        None
    } else {
        Some(context.query_result_iterator::<School,_>((
            GeographyProperties {
                fips_code: school_string.parse().map_err(|_| IxaError::IxaError(format!("Invalid FIPS code: {}", school_string)))?,
            },
        )).next().ok_or_else(|| IxaError::IxaError(format!("No setting found with string: {}", school_string)))?)
    };

    let workplace_setting_id: Option<WorkplaceId> = if workplace_string.is_empty() {
        None
    } else {
        Some(context.query_result_iterator::<Workplace,_>((
            GeographyProperties {
                fips_code: workplace_string.parse().map_err(|_| IxaError::IxaError(format!("Invalid FIPS code: {}", workplace_string)))?,
            },
        )).next().ok_or_else(|| IxaError::IxaError(format!("No setting found with string: {}", workplace_string)))?)
    };
    
    // Add person to context
    let person_id: PersonId = context
        .add_entity((
            Age(person_record.age),
            PersonHomeId(home_setting_id),
            PersonSchoolId(school_setting_id),
            PersonWorkplaceId(workplace_setting_id),
            PersonCensusTractId(tract_setting_id),
        ))
        .unwrap();
    context.set_property(person_id, ItineraryEntry {
        home_ratio: Some(*itinerary_ratios.get(&DefaultSettingCategory::Home).unwrap_or(&0.0)),
        school_ratio: school_setting_id.map(|_| *itinerary_ratios.get(&DefaultSettingCategory::School).unwrap_or(&0.0)),
        workplace_ratio: workplace_setting_id.map(|_| *itinerary_ratios.get(&DefaultSettingCategory::Workplace).unwrap_or(&0.0)),
        census_tract_ratio: Some(*itinerary_ratios.get(&DefaultSettingCategory::CensusTract).unwrap_or(&0.0)),
    });
    Ok(())
}

fn load_synth_population(context: &mut Context, synth_input_file: PathBuf, itinerary_ratios: HashMap<DefaultSettingCategory, f64>) -> Result<(), IxaError> {
    let mut reader = csv::Reader::from_path(synth_input_file)?;
    let mut raw_record = csv::ByteRecord::new();
    let headers = reader.byte_headers()?.clone();

    while reader.read_byte_record(&mut raw_record)? {
        let record: PeopleRecord = raw_record.deserialize(Some(&headers))?;
        create_person_from_record(context, &record, itinerary_ratios.clone())?;
    }
    Ok(())
}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    let _span = open_span("load_synth_population");
    let Params {
        synth_population_file,
        itinerary_ratios,
        ..
    } = context.get_params();
    load_synth_population(context, synth_population_file.clone(), itinerary_ratios.clone())?;
    context.index_property::<Person, PersonHomeId>();
    context.index_property::<Person, PersonSchoolId>();
    context.index_property::<Person, PersonWorkplaceId>();
    context.index_property::<Person, PersonCensusTractId>();
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::parameters::GlobalParams;
    use crate::setting_loader::{DefaultSettingCategory, SettingCategory, SettingEntityProperties, load_settings};
    use crate::settings_entities::ContextSettingExt;
    use ixa::{ContextGlobalPropertiesExt, HashMap};
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;

    fn persist_tmp_csv(content: &String) -> PathBuf {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        let (_file, path) = file.keep().unwrap();
        path
    }

    fn setup() -> Context {
        let mut context = Context::new();
        let parameters = Params {
            // We need to specify an itinerary split here even though we don't draw people from
            // itineraries because `load_synth_population` calls `create_itinerary` for each person,
            // and that function requires an itinerary write function to be set.
            settings_properties: HashMap::from_iter(
                [
                    (
                        DefaultSettingCategory::Home,
                        SettingEntityProperties { alpha: 0.0 },
                    ),
                    (
                        DefaultSettingCategory::School,
                        SettingEntityProperties { alpha: 0.0 },
                    ),
                    (
                        DefaultSettingCategory::Workplace,
                        SettingEntityProperties { alpha: 0.0 },
                    ),
                    (
                        DefaultSettingCategory::CensusTract,
                        SettingEntityProperties { alpha: 0.0 },
                    ),
                ]
                .into_iter()
                .collect::<HashMap<_, _>>(),
            ),
            itinerary_ratios: HashMap::from_iter([
                (DefaultSettingCategory::Home, 0.25),
                (DefaultSettingCategory::School, 0.25),
                (DefaultSettingCategory::Workplace, 0.25),
                (DefaultSettingCategory::CensusTract, 0.25),
            ]),
            ..Default::default()
        };
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();
        // crate::settings::init(&mut context);
        context
    }

    #[test]
    fn check_synth_file_tract() {
        let mut context = setup();

        let (settings_properties, itinerary_ratios) = {
            let Params {
                settings_properties,
                itinerary_ratios,
                ..
            } = context.get_params();
            (settings_properties.clone(), itinerary_ratios.clone())
        };

        let setting_input = String::from(
            "setting_category,setting_code\nhomeId,360930331020001\nhomeId,360930331020002\ncensustractId,36093033102",
        );
        let setting_file = persist_tmp_csv(&setting_input);
        load_settings(
            &mut context,
            setting_file,
            settings_properties,
            itinerary_ratios.clone(),
        )
        .unwrap();

        let input = String::from(
            "age,homeId,schoolId,workplaceId\n43,360930331020001,,\n42,360930331020002,,",
        );
        let synth_file = persist_tmp_csv(&input);
        load_synth_population(&mut context, synth_file, itinerary_ratios.clone()).unwrap();

        let age = [43, 42];
        let home_id = [360_930_331_020_001, 360_930_331_020_002];
        let census_tract_id = 36_093_033_102;

        assert_eq!(context.get_entity_count::<Person>(), 2);
        assert_eq!(context.get_entity_count::<Home>(), 3);
        assert_eq!(context.get_entity_count::<CensusTract>(), 1);

        for item in age.iter().take(1) {
            assert_eq!(1, context.query_entity_count::<Person, _>((Age(*item),)));
        }

        let home_setting_id_1 = context
            .query_result_iterator::<Home, _>((
                GeographyProperties {
                    fips_code: home_id[0],
                },
            ))
            .next()
            .unwrap();
        let home_setting_id_2 = context
            .query_result_iterator::<Home, _>((
                GeographyProperties {
                    fips_code: home_id[1],
                },
            ))
            .next()
            .unwrap();
        let census_tract_setting_id = context
            .query_result_iterator::<CensusTract, _>((
                GeographyProperties {
                    fips_code: census_tract_id,
                },
            ))
            .next()
            .unwrap();

        println!("home_setting_id_1: {:?}", home_setting_id_1);
        println!("home_setting_id_2: {:?}", home_setting_id_2);
        println!("census_tract_setting_id: {:?}", census_tract_setting_id);

        assert_eq!(1, context.get_setting_members(SettingCategory::Home(home_setting_id_1)).count());

        assert_eq!(1, context.get_setting_members(SettingCategory::Home(home_setting_id_2)).count());

        assert_eq!(
            2,
            context.get_setting_members(SettingCategory::CensusTract(census_tract_setting_id)).count()
        );
    }

    #[test]
    #[should_panic(expected = "range end index 11 out of range for slice of length 9")]
    fn check_invalid_census_tract() {
        let mut context = setup();
        let itinerary_ratios= {
            let Params {
                itinerary_ratios,
                ..
            } = context.get_params();
            itinerary_ratios.clone()
        };
        let input =
            String::from("age,homeId,schoolId,workplaceId\n43,360930331,,\n42,360930331020002,,");
        let synth_file = persist_tmp_csv(&input);
        load_synth_population(&mut context, synth_file, itinerary_ratios.clone()).unwrap();
    }

    #[test]
    fn check_synth_file_school() {
        let mut context = setup();

        let (settings_properties, itinerary_ratios) = {
            let Params {
                settings_properties,
                itinerary_ratios,
                ..
            } = context.get_params();
            (settings_properties.clone(), itinerary_ratios.clone())
        };

        let setting_input = String::from(
            "setting_category,setting_code\nhomeId,360930331020001\nhomeId,360930331020002\ncensustractId,36093033102\nschoolId,1\nschoolId,2",
        );
        let setting_file = persist_tmp_csv(&setting_input);
        load_settings(
            &mut context,
            setting_file,
            settings_properties,
            itinerary_ratios.clone(),
        )
        .unwrap();

        let input = String::from(
            "age,homeId,schoolId,workplaceId\n43,360930331020001,1,\n42,360930331020002,2,",
        );
        let synth_file = persist_tmp_csv(&input);
        load_synth_population(&mut context, synth_file, itinerary_ratios.clone()).unwrap();
        let age = [43, 42];
        let school_id = [1, 2];
        let home_id = [360_930_331_020_001, 360_930_331_020_002];
        let census_tract_id = 36_093_033_102;

        assert_eq!(context.get_entity_count::<Person>(), 2);
        assert_eq!(context.get_entity_count::<Home>(), 2);
        assert_eq!(context.get_entity_count::<CensusTract>(), 1);
        assert_eq!(context.get_entity_count::<School>(), 2);

        let home_setting_id_1 = context
            .query_result_iterator::<Home, _>((
                GeographyProperties {
                    fips_code: home_id[0],
                },
            ))
            .next()
            .unwrap();
        let home_setting_id_2 = context
            .query_result_iterator::<Home, _>((
                GeographyProperties {
                    fips_code: home_id[1],
                },
            ))
            .next()
            .unwrap();
        let census_tract_setting_id = context
            .query_result_iterator::<CensusTract, _>((
                GeographyProperties {
                    fips_code: census_tract_id,
                },
            ))
            .next()
            .unwrap();
        let school_setting_id_1 = context
            .query_result_iterator::<School, _>((
                GeographyProperties {
                    fips_code: school_id[0],
                },
            ))
            .next()
            .unwrap();
        let school_setting_id_2 = context
            .query_result_iterator::<School, _>((
                GeographyProperties {
                    fips_code: school_id[1],
                },
            ))
            .next()
            .unwrap();

        for item in age.iter().take(1) {
            assert_eq!(1, context.query_entity_count::<Person, _>((Age(*item),)));
        }
        assert_eq!(1, context.get_setting_members(SettingCategory::School(school_setting_id_1)).count());
        assert_eq!(1, context.get_setting_members(SettingCategory::Home(home_setting_id_1)).count());
        assert_eq!(1, context.get_setting_members(SettingCategory::School(school_setting_id_2)).count());
        assert_eq!(1, context.get_setting_members(SettingCategory::Home(home_setting_id_2)).count());
        assert_eq!(
            2,
            context.get_setting_members(SettingCategory::CensusTract(census_tract_setting_id)).count()
        );
    }

    #[test]
    fn check_synth_file_workplace() {
        let mut context = setup();

        let (settings_properties, itinerary_ratios) = {
            let Params {
                settings_properties,
                itinerary_ratios,
                ..
            } = context.get_params();
            (settings_properties.clone(), itinerary_ratios.clone())
        };

        let setting_input = String::from(
            "setting_category,setting_code\nhomeId,360930331020001\nhomeId,360930331020002\ncensustractId,36093033102\nworkplaceId,1\nworkplaceId,2",
        );
        let setting_file = persist_tmp_csv(&setting_input);
        load_settings(
            &mut context,
            setting_file,
            settings_properties,
            itinerary_ratios.clone(),
        )
        .unwrap();

        let input = String::from(
            "age,homeId,schoolId,workplaceId\n43,360930331020001,,1\n42,360930331020002,,2",
        );
        let synth_file = persist_tmp_csv(&input);
        load_synth_population(&mut context, synth_file, itinerary_ratios.clone()).unwrap();
        let age = [43, 42];
        let workplace_id = [1, 2];
        let home_id = [360_930_331_020_001, 360_930_331_020_002];
        let census_tract_id = 36_093_033_102;

        assert_eq!(context.get_entity_count::<Person>(), 2);
        assert_eq!(context.get_entity_count::<Home>(), 2);
        assert_eq!(context.get_entity_count::<CensusTract>(), 1);
        assert_eq!(context.get_entity_count::<Workplace>(), 2);

        let home_setting_id_1 = context
            .query_result_iterator::<Home, _>((
                GeographyProperties {
                    fips_code: home_id[0],
                },
            ))
            .next()
            .unwrap();
        let home_setting_id_2 = context
            .query_result_iterator::<Home, _>((
                GeographyProperties {
                    fips_code: home_id[1],
                },
            ))
            .next()
            .unwrap();
        let census_tract_setting_id = context
            .query_result_iterator::<CensusTract, _>((
                GeographyProperties {
                    fips_code: census_tract_id,
                },
            ))
            .next()
            .unwrap();
        let workplace_setting_id_1 = context
            .query_result_iterator::<Workplace, _>((
                GeographyProperties {
                    fips_code: workplace_id[0],
                },
            ))
            .next()
            .unwrap();
        let workplace_setting_id_2 = context
            .query_result_iterator::<Workplace, _>((
                GeographyProperties {
                    fips_code: workplace_id[1],
                },
            ))
            .next()
            .unwrap();

        for item in age.iter().take(1) {
            assert_eq!(1, context.query_entity_count::<Person, _>((Age(*item),)));
        }
        assert_eq!(
            1,
            context.get_setting_members(SettingCategory::Workplace(workplace_setting_id_1)).count()
        );
        assert_eq!(1, context.get_setting_members(SettingCategory::Home(home_setting_id_1)).count());
        assert_eq!(
            1,
            context.get_setting_members(SettingCategory::Workplace(workplace_setting_id_2)).count()
        );
        assert_eq!(1, context.get_setting_members(SettingCategory::Home(home_setting_id_2)).count());
        assert_eq!(
            1,
            context.get_setting_members(SettingCategory::Workplace(workplace_setting_id_2)).count()
        );
        assert_eq!(
            2,
            context.get_setting_members(SettingCategory::CensusTract(census_tract_setting_id)).count()
        );
    }
}
