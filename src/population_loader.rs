use ixa::{csv, prelude::*};

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::{
    parameters::{ContextParametersExt, Params},
    settings_entities::{Alpha, CensusTractCode, CountyCode, DefaultItineraryRatio, Setting, SettingCategory, SettingCode, SettingId, StateCode},
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
pub struct HomeId(pub SettingId);
impl_property!(HomeId, Person);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct SchoolId(pub Option<SettingId>);
impl_property!(SchoolId, Person);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct WorkplaceId(pub Option<SettingId>);
impl_property!(WorkplaceId, Person);
#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct CensusTractId(pub SettingId);
impl_property!(CensusTractId, Person);

fn create_person_from_record(
    context: &mut Context,
    person_record: &PeopleRecord,
) -> Result<(), IxaError> {
    // Create itinerary entries for all setting memberships in input file
    let tract: String = String::from_utf8(person_record.homeId[..11].to_owned())?;
    let home_id: String = String::from_utf8(person_record.homeId.to_owned())?;
    let school_string: String = String::from_utf8(person_record.schoolId.to_owned())?;
    let workplace_string: String = String::from_utf8(person_record.workplaceId.to_owned())?;

    let home_setting_id: SettingId =
        get_setting_id_from_string(context, SettingCategory::Home, home_id.clone())?;
    let tract_setting_id: SettingId =
        get_setting_id_from_string(context, SettingCategory::CensusTract, tract.clone())?;
    let school_setting_id: Option<SettingId> = if school_string.is_empty() {
        None
    } else {
        Some(get_setting_id_from_string(
            context,
            SettingCategory::School,
            school_string.clone(),
        )?)
    };
    let workplace_setting_id: Option<SettingId> = if workplace_string.is_empty() {
        None
    } else {
        Some(get_setting_id_from_string(
            context,
            SettingCategory::Workplace,
            workplace_string.clone(),
        )?)
    };

    // Add person to context
    let _person_id: PersonId = context
        .add_entity((
            Age(person_record.age),
            HomeId(home_setting_id),
            SchoolId(school_setting_id),
            WorkplaceId(workplace_setting_id),
            CensusTractId(tract_setting_id),
        ))
        .unwrap();
    Ok(())
}

fn get_setting_id_from_string(
    context: &mut Context,
    setting_category: SettingCategory,
    setting_string: String,
) -> Result<SettingId, IxaError> {
    println!("Getting setting ID for category {:?} and string {}", setting_category, setting_string);
    let setting_code: usize = setting_string
        .parse()
        .map_err(|_| IxaError::IxaError(format!("Invalid FIPS code: {}", setting_string)))?;
    if let Some(setting_id) = context
        .query_result_iterator::<Setting, _>((SettingCode(setting_code), setting_category))
        .next() {
            return Ok(setting_id);
        }
    let fips = get_fips_from_string(setting_string.clone())?;
    let alpha = *get_default_setting_properties(context, setting_category)?;
    let itinerary_ratio = get_default_itinerary_ratio(context, setting_category)?;
    println!("Itinerary ratio: {}", itinerary_ratio);
    println!("Default alpha: {:?}", alpha);
    println!("Creating setting with code {} and category {:?} with FIPS {:?}", setting_code, setting_category, fips);
    let setting_id: SettingId = context
        .add_entity::<Setting, _>((
            SettingCode(setting_code),
            StateCode(fips.0),
            CountyCode(fips.1),
            CensusTractCode(fips.2),
            setting_category,
            Alpha(alpha),
            DefaultItineraryRatio(itinerary_ratio),
        ))?;
    // context.set_property::<Setting, Alpha>(setting_id, Alpha(alpha));
    // context.set_property::<Setting, DefaultItineraryRatio>(setting_id, DefaultItineraryRatio(itinerary_ratio));
    Ok(setting_id)
}

fn get_fips_from_string(setting_string: String) -> Result<(usize, usize, usize), IxaError> {
    if setting_string.len() < 11 {
        return Err(IxaError::IxaError(format!("Invalid FIPS code length: {}", setting_string)));
    }
    let state_code: usize = setting_string[0..2]
        .parse()
        .map_err(|_| IxaError::IxaError(format!("Invalid state code in FIPS: {}", setting_string)))?;
    let county_code: usize = setting_string[2..5]
        .parse()
        .map_err(|_| IxaError::IxaError(format!("Invalid county code in FIPS: {}", setting_string)))?;
    let tract_code: usize = setting_string[5..11]
        .parse()
        .map_err(|_| IxaError::IxaError(format!("Invalid tract code in FIPS: {}", setting_string)))?;
    Ok((state_code, county_code, tract_code))
}

fn get_default_setting_properties(
    context: &Context,
    setting_category: SettingCategory,
) -> Result<&f64, IxaError> {
    let Params {
        settings_properties,
        ..
    } = context.get_params();
    settings_properties
        .get(&setting_category)
        .ok_or_else(|| IxaError::IxaError(format!("No properties found for setting category: {:?}", setting_category)))
}

fn get_default_itinerary_ratio(context: &Context, setting_category: SettingCategory) -> Result<f64, IxaError> {
    let Params {
        itinerary_ratios,
        ..
    } = context.get_params();
    itinerary_ratios
        .get(&setting_category)
        .cloned()
        .ok_or_else(|| IxaError::IxaError(format!("No itinerary ratio found for setting category: {:?}", setting_category)))
}

fn load_synth_population(context: &mut Context, synth_input_file: PathBuf) -> Result<(), IxaError> {
    let mut reader = csv::Reader::from_path(synth_input_file)?;
    let mut raw_record = csv::ByteRecord::new();
    let headers = reader.byte_headers()?.clone();

    while reader.read_byte_record(&mut raw_record)? {
        let record: PeopleRecord = raw_record.deserialize(Some(&headers))?;
        create_person_from_record(context, &record)?;
    }
    Ok(())
}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    let _span = open_span("load_synth_population");
    let Params {
        synth_population_file,
        ..
    } = context.get_params();
    load_synth_population(context, synth_population_file.clone())?;
    context.index_property::<Person, HomeId>();
    context.index_property::<Person, SchoolId>();
    context.index_property::<Person, WorkplaceId>();
    context.index_property::<Person, CensusTractId>();
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::parameters::GlobalParams;
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
                        SettingCategory::Home,
                        0.0,
                    ),
                    (
                        SettingCategory::School,
                        0.0,
                    ),
                    (
                        SettingCategory::Workplace,
                        0.0,
                    ),
                    (
                        SettingCategory::CensusTract,
                        0.0,
                    ),
                ]
                .into_iter()
                .collect::<HashMap<_, _>>(),
            ),
            itinerary_ratios: HashMap::from_iter([
                (SettingCategory::Home, 0.25),
                (SettingCategory::School, 0.25),
                (SettingCategory::Workplace, 0.25),
                (SettingCategory::CensusTract, 0.25),
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

        let input = String::from(
            "age,homeId,schoolId,workplaceId\n43,360930331020001,,\n42,360930331020002,,",
        );
        let synth_file = persist_tmp_csv(&input);
        load_synth_population(&mut context, synth_file).unwrap();

        let age = [43, 42];
        let home_id = [360_930_331_020_001, 360_930_331_020_002];
        let census_tract_id = 36_093_033_102;

        assert_eq!(context.get_entity_count::<Person>(), 2);
        assert_eq!(context.get_entity_count::<Setting>(), 3);

        for item in age.iter().take(1) {
            assert_eq!(1, context.query_entity_count::<Person, _>((Age(*item),)));
        }

        let home_setting_id_1 = context
            .query_result_iterator::<Setting, _>((
                SettingCode(home_id[0]),
                SettingCategory::Home,
            ))
            .next()
            .unwrap();
        let home_setting_id_2 = context
            .query_result_iterator::<Setting, _>((
                SettingCode(home_id[1]),
                SettingCategory::Home,
            ))
            .next()
            .unwrap();
        let census_tract_setting_id = context
            .query_result_iterator::<Setting, _>((
                SettingCode(census_tract_id),
                SettingCategory::CensusTract,
            ))
            .next()
            .unwrap();

        println!("home_setting_id_1: {:?}", home_setting_id_1);
        println!("home_setting_id_2: {:?}", home_setting_id_2);
        println!("census_tract_setting_id: {:?}", census_tract_setting_id);

        assert_eq!(1, context.get_setting_members(home_setting_id_1).count());

        assert_eq!(1, context.get_setting_members(home_setting_id_2).count());

        assert_eq!(
            2,
            context.get_setting_members(census_tract_setting_id).count()
        );
    }

    #[test]
    #[should_panic(expected = "range end index 11 out of range for slice of length 9")]
    fn check_invalid_census_tract() {
        let mut context = setup();
        let input =
            String::from("age,homeId,schoolId,workplaceId\n43,360930331,,\n42,360930331020002,,");
        let synth_file = persist_tmp_csv(&input);
        load_synth_population(&mut context, synth_file).unwrap();
    }

    #[test]
    fn check_synth_file_school() {
        let mut context = setup();

        let input = String::from(
            "age,homeId,schoolId,workplaceId\n43,360930331020001,1,\n42,360930331020002,2,",
        );
        let synth_file = persist_tmp_csv(&input);
        load_synth_population(&mut context, synth_file).unwrap();
        let age = [43, 42];
        let school_id = [1, 2];
        let home_id = [360_930_331_020_001, 360_930_331_020_002];
        let census_tract_id = 36_093_033_102;

        assert_eq!(context.get_entity_count::<Person>(), 2);
        assert_eq!(context.get_entity_count::<Setting>(), 5);

        let home_setting_id_1 = context
            .query_result_iterator::<Setting, _>((
                SettingCode(home_id[0]),
                SettingCategory::Home,
            ))
            .next()
            .unwrap();
        let home_setting_id_2 = context
            .query_result_iterator::<Setting, _>((
                SettingCode(home_id[1]),
                SettingCategory::Home,
            ))
            .next()
            .unwrap();
        let census_tract_setting_id = context
            .query_result_iterator::<Setting, _>((
                SettingCode(census_tract_id),
                SettingCategory::CensusTract,
            ))
            .next()
            .unwrap();
        let school_setting_id_1 = context
            .query_result_iterator::<Setting, _>((
                SettingCode(school_id[0]),
                SettingCategory::School,
            ))
            .next()
            .unwrap();
        let school_setting_id_2 = context
            .query_result_iterator::<Setting, _>((
                SettingCode(school_id[1]),
                SettingCategory::School,
            ))
            .next()
            .unwrap();

        for item in age.iter().take(1) {
            assert_eq!(1, context.query_entity_count::<Person, _>((Age(*item),)));
        }
        assert_eq!(1, context.get_setting_members(school_setting_id_1).count());
        assert_eq!(1, context.get_setting_members(home_setting_id_1).count());
        assert_eq!(1, context.get_setting_members(school_setting_id_2).count());
        assert_eq!(1, context.get_setting_members(home_setting_id_2).count());
        assert_eq!(
            2,
            context.get_setting_members(census_tract_setting_id).count()
        );
    }

    #[test]
    fn check_synth_file_workplace() {
        let mut context = setup();

        let input = String::from(
            "age,homeId,schoolId,workplaceId\n43,360930331020001,,1\n42,360930331020002,,2",
        );
        let synth_file = persist_tmp_csv(&input);
        load_synth_population(&mut context, synth_file).unwrap();
        let age = [43, 42];
        let workplace_id = [1, 2];
        let home_id = [360_930_331_020_001, 360_930_331_020_002];
        let census_tract_id = 36_093_033_102;

        assert_eq!(context.get_entity_count::<Person>(), 2);
            assert_eq!(context.get_entity_count::<Setting>(), 5);

        let home_setting_id_1 = context
            .query_result_iterator::<Setting, _>((
                SettingCode(home_id[0]),
                SettingCategory::Home,
            ))
            .next()
            .unwrap();
        let home_setting_id_2 = context
            .query_result_iterator::<Setting, _>((
                SettingCode(home_id[1]),
                SettingCategory::Home,
            ))
            .next()
            .unwrap();
        let census_tract_setting_id = context
            .query_result_iterator::<Setting, _>((
                SettingCode(census_tract_id),
                SettingCategory::CensusTract,
            ))
            .next()
            .unwrap();
        let workplace_setting_id_1 = context
            .query_result_iterator::<Setting, _>((
                SettingCode(workplace_id[0]),
                SettingCategory::Workplace,
            ))
            .next()
            .unwrap();
        let workplace_setting_id_2 = context
            .query_result_iterator::<Setting, _>((
                SettingCode(workplace_id[1]),
                SettingCategory::Workplace,
            ))
            .next()
            .unwrap();

        for item in age.iter().take(1) {
            assert_eq!(1, context.query_entity_count::<Person, _>((Age(*item),)));
        }
        assert_eq!(
            1,
            context.get_setting_members(workplace_setting_id_1).count()
        );
        assert_eq!(1, context.get_setting_members(home_setting_id_1).count());
        assert_eq!(
            1,
            context.get_setting_members(workplace_setting_id_2).count()
        );
        assert_eq!(1, context.get_setting_members(home_setting_id_2).count());
        assert_eq!(
            1,
            context.get_setting_members(workplace_setting_id_2).count()
        );
        assert_eq!(
            2,
            context.get_setting_members(census_tract_setting_id).count()
        );
    }
}
