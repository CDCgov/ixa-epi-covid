use ixa::{csv, prelude::*};

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::{
    itinerary::{Activity, BelongsTo, CensusTractItinerary, HomeItinerary, Itinerary, ItineraryCensusTractId, ItineraryHomeId, ItinerarySchoolId, ItineraryWorkplaceId, SchoolItinerary, WorkplaceItinerary}, parameters::{ContextParametersExt, Params}, settings_entities::{Alpha, CensusTract, DefaultItineraryRatio, DefaultSettingCategory, Home, SettingCategory, SettingCode, StateCode, Workplace}
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

fn create_person_from_record(
    context: &mut Context,
    person_record: &PeopleRecord,
) -> Result<(), IxaError> {
    // Create itinerary entries for all setting memberships in input file
    let tract: String = String::from_utf8(person_record.homeId[..11].to_owned())?;
    let home_id: String = String::from_utf8(person_record.homeId.to_owned())?;
    let school_string: String = String::from_utf8(person_record.schoolId.to_owned())?;
    let workplace_string: String = String::from_utf8(person_record.workplaceId.to_owned())?;

    let home: SettingCategory = add_setting(context, DefaultSettingCategory::Home, home_id)?;
    let tract: SettingCategory = add_setting(context, DefaultSettingCategory::CensusTract, tract)?;
    let school_setting_id: Option<SettingCategory> = if school_string.is_empty() {
        None
    } else {
        Some(add_setting(context, DefaultSettingCategory::School, school_string)?)
    };
    let workplace_setting_id: Option<SettingCategory> = if workplace_string.is_empty() {
        None
    } else {
        Some(add_setting(
            context,
            DefaultSettingCategory::Workplace,
            workplace_string.clone(),
        )?)
    };
    // Add person to context
    let person_id: PersonId = context
        .add_entity((
            Age(person_record.age),
        ))
        .unwrap();
    let _itinerary_id = context
        .add_entity((
            BelongsTo(person_id),
            HomeItinerary{home_id: match home { SettingCategory::Home(id) => id, _ => unreachable!() }, ratio: get_default_itinerary_ratio(context, DefaultSettingCategory::Home)?},
            SchoolItinerary{school_id: match school_setting_id { Some(SettingCategory::School(id)) => Some(id), _ => None }, ratio: Some(get_default_itinerary_ratio(context, DefaultSettingCategory::School)?)},
            WorkplaceItinerary{workplace_id: match workplace_setting_id { Some(SettingCategory::Workplace(id)) => Some(id), _ => None }, ratio: Some(get_default_itinerary_ratio(context, DefaultSettingCategory::Workplace)?)},
            CensusTractItinerary{census_tract_id: match tract { SettingCategory::CensusTract(id) => id, _ => unreachable!() }, ratio: get_default_itinerary_ratio(context, DefaultSettingCategory::CensusTract)?},
        ))
        .unwrap();  
    Ok(())
}

fn add_setting(
    context: &mut Context,
    setting_category: DefaultSettingCategory,
    setting_string: String,
) -> Result<SettingCategory, IxaError> {
    println!("code is {}", setting_string);
    let setting_code: usize = setting_string
        .parse()
        .map_err(|_| IxaError::IxaError(format!("Invalid FIPS code: {}", setting_string)))?;

    if let Some(setting) = match setting_category {
        DefaultSettingCategory::Home => {
            if let Some(setting_id) = context
                .query_result_iterator::<Home, _>((SettingCode(setting_code),))
                .next() {
                Some(SettingCategory::Home(setting_id))
            } else {
                let fips = get_fips_from_string(setting_string.clone())?;
                let alpha = *get_default_setting_properties(context, setting_category)?;
                let itinerary_ratio = get_default_itinerary_ratio(context, setting_category)?;
                Some(SettingCategory::Home(context.add_entity::<Home, _>((
                    SettingCode(setting_code),
                    StateCode(fips.0),
                    Alpha(alpha),
                    DefaultItineraryRatio(itinerary_ratio),
                ))?))
            }
        }
        DefaultSettingCategory::School => {
            if let Some(setting_id) = context
                .query_result_iterator::<crate::settings_entities::School, _>((SettingCode(setting_code),))
                .next() {
                Some(SettingCategory::School(setting_id))
            } else {
                let fips = get_fips_from_string(setting_string.clone())?;
                let alpha = *get_default_setting_properties(context, setting_category)?;
                let itinerary_ratio = get_default_itinerary_ratio(context, setting_category)?;
                Some(SettingCategory::School(context.add_entity::<crate::settings_entities::School, _>((
                    SettingCode(setting_code),
                    StateCode(fips.0),
                    Alpha(alpha),
                    DefaultItineraryRatio(itinerary_ratio),
                ))?))
            }
        }
        DefaultSettingCategory::Workplace => {
            if let Some(setting_id) = context
                .query_result_iterator::<Workplace, _>((SettingCode(setting_code),))
                .next() {
                Some(SettingCategory::Workplace(setting_id))
            } else {
                let fips = get_fips_from_string(setting_string.clone())?;
                let alpha = *get_default_setting_properties(context, setting_category)?;
                let itinerary_ratio = get_default_itinerary_ratio(context, setting_category)?;
                Some(SettingCategory::Workplace(context.add_entity::<Workplace, _>((
                    SettingCode(setting_code),
                    StateCode(fips.0),
                    Alpha(alpha),
                    DefaultItineraryRatio(itinerary_ratio),
                ))?))
            }
        }
        DefaultSettingCategory::CensusTract => {
            if let Some(setting_id) = context
                .query_result_iterator::<CensusTract, _>((SettingCode(setting_code),))
                .next() {
                Some(SettingCategory::CensusTract(setting_id))
            } else {
                let fips = get_fips_from_string(setting_string.clone())?;
                let alpha = *get_default_setting_properties(context, setting_category)?;
                let itinerary_ratio = get_default_itinerary_ratio(context, setting_category)?;
                Some(SettingCategory::CensusTract(context.add_entity::<CensusTract, _>((
                    SettingCode(setting_code),
                    StateCode(fips.0),
                    Alpha(alpha),
                    DefaultItineraryRatio(itinerary_ratio),
                ))?))
            }

                
        }
    } {
        return Ok(setting);
    }
    let fips = get_fips_from_string(setting_string.clone())?;
    let alpha = *get_default_setting_properties(context, setting_category)?;
    let itinerary_ratio = get_default_itinerary_ratio(context, setting_category)?;
    let setting_id = match setting_category {
        DefaultSettingCategory::Home => SettingCategory::Home(context.add_entity::<Home, _>((
            SettingCode(setting_code),
            StateCode(fips.0),
            Alpha(alpha),
            DefaultItineraryRatio(itinerary_ratio),
        ))?),
        DefaultSettingCategory::School => SettingCategory::School(context.add_entity::<crate::settings_entities::School, _>((
            SettingCode(setting_code),
            StateCode(fips.0),
            Alpha(alpha),
            DefaultItineraryRatio(itinerary_ratio),
        ))?),
        DefaultSettingCategory::Workplace => SettingCategory::Workplace(context.add_entity::<Workplace, _>((
            SettingCode(setting_code),
            StateCode(fips.0),
            Alpha(alpha),
            DefaultItineraryRatio(itinerary_ratio),
        ))?),
        DefaultSettingCategory::CensusTract => SettingCategory::CensusTract(context.add_entity::<CensusTract, _>((
            SettingCode(setting_code),
            StateCode(fips.0),
            Alpha(alpha),
            DefaultItineraryRatio(itinerary_ratio),
        ))?),
    };
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
    setting_category: DefaultSettingCategory,
) -> Result<&f64, IxaError> {
    let Params {
        settings_properties,
        ..
    } = context.get_params();
    settings_properties
        .get(&setting_category)
        .ok_or_else(|| IxaError::IxaError(format!("No properties found for setting category: {:?}", setting_category)))
}

fn get_default_itinerary_ratio(context: &Context, setting_category: DefaultSettingCategory) -> Result<f64, IxaError> {
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
    context.index_property::<Itinerary, ItineraryHomeId>();
    context.index_property::<Itinerary, ItinerarySchoolId>();
    context.index_property::<Itinerary, ItineraryWorkplaceId>();
    context.index_property::<Itinerary, ItineraryCensusTractId>();
    context.index_property::<Itinerary, (BelongsTo, Activity)>();
    context.index_property::<Itinerary, BelongsTo>();
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::itinerary::{ContextItineraryExt, Itinerary};
    use crate::parameters::GlobalParams;
    use crate::settings_entities::School;
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
                        0.0,
                    ),
                    (
                        DefaultSettingCategory::School,
                        0.0,
                    ),
                    (
                        DefaultSettingCategory::Workplace,
                        0.0,
                    ),
                    (
                        DefaultSettingCategory::CensusTract,
                        0.0,
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

        let input = String::from(
            "age,homeId,schoolId,workplaceId\n43,360930331020001,,\n42,360930331020002,,",
        );
        let synth_file = persist_tmp_csv(&input);
        load_synth_population(&mut context, synth_file).unwrap();

        let age = [43, 42];
        let home_id = [360_930_331_020_001, 360_930_331_020_002];
        let census_tract_id = 36_093_033_102;

        assert_eq!(context.get_entity_count::<Person>(), 2);
        assert_eq!(context.get_entity_count::<Home>(), 2);
        assert_eq!(context.get_entity_count::<CensusTract>(), 1);
        assert_eq!(context.get_entity_count::<Itinerary>(), 2);

        for item in age.iter().take(1) {
            assert_eq!(1, context.query_entity_count::<Person, _>((Age(*item),)));
        }

        let home_setting_id_1 = context
            .query_result_iterator::<Home, _>((
                SettingCode(home_id[0]),
            ))
            .next()
            .unwrap();
        let home_setting_id_2 = context
            .query_result_iterator::<Home, _>((
                SettingCode(home_id[1]),
            ))
            .next()
            .unwrap();
        let census_tract_setting_id = context
            .query_result_iterator::<CensusTract, _>((
                SettingCode(census_tract_id),
            ))
            .next()
            .unwrap();

        assert_eq!(1, context.get_setting_members(SettingCategory::Home(home_setting_id_1)).len());

        assert_eq!(1, context.get_setting_members(SettingCategory::Home(home_setting_id_2)).len());

        assert_eq!(
            2,
            context.get_setting_members(SettingCategory::CensusTract(census_tract_setting_id)).len()
        );

        let p1 = context.query_result_iterator::<Person, _>((Age(43),)).next().unwrap();
        let itinerary_id_1 = context
            .query_result_iterator::<Itinerary, _>((BelongsTo(p1),))
            .next()
            .unwrap();
        assert_eq!(
            context.get_property::<Itinerary, ItineraryHomeId>(itinerary_id_1).0,
            home_setting_id_1
        );
        assert_eq!(
            context.get_property::<Itinerary, ItineraryCensusTractId>(itinerary_id_1).0,
            census_tract_setting_id
        );
        assert_eq!(
            context.get_property::<Itinerary, ItinerarySchoolId>(itinerary_id_1).0,
            None
        );
        assert_eq!(
            context.get_property::<Itinerary, ItineraryWorkplaceId>(itinerary_id_1).0,
            None
        );
        assert!(context.get_property::<Itinerary, Activity>(itinerary_id_1).0);



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
            "age,homeId,schoolId,workplaceId\n43,360930331020001,360930331020003,\n42,360930331020002,360930331020004,",
        );
        let synth_file = persist_tmp_csv(&input);
        load_synth_population(&mut context, synth_file).unwrap();
        let age = [43, 42];
        let school_id = [360_930_331_020_003, 360_930_331_020_004];
        let home_id = [360_930_331_020_001, 360_930_331_020_002];
        let census_tract_id = 36_093_033_102;

        assert_eq!(context.get_entity_count::<Person>(), 2);
        assert_eq!(context.get_entity_count::<Home>(), 2);
        assert_eq!(context.get_entity_count::<School>(), 2);
        assert_eq!(context.get_entity_count::<CensusTract>(), 1);

        let home_setting_id_1 = context
            .query_result_iterator::<Home, _>((
                SettingCode(home_id[0]),
            ))
            .next()
            .unwrap();
        let home_setting_id_2 = context
            .query_result_iterator::<Home, _>((
                SettingCode(home_id[1]),
            ))
            .next()
            .unwrap();
        let census_tract_setting_id = context
            .query_result_iterator::<CensusTract, _>((
                SettingCode(census_tract_id),
            ))
            .next()
            .unwrap();
        let school_setting_id_1 = context
            .query_result_iterator::<School, _>((
                SettingCode(school_id[0]),
            ))
            .next()
            .unwrap();
        let school_setting_id_2 = context
            .query_result_iterator::<School, _>((
                SettingCode(school_id[1]),
            ))
            .next()
            .unwrap();

        for item in age.iter().take(1) {
            assert_eq!(1, context.query_entity_count::<Person, _>((Age(*item),)));
        }
        assert_eq!(1, context.get_setting_members(SettingCategory::School(school_setting_id_1)).len());
        assert_eq!(1, context.get_setting_members(SettingCategory::Home(home_setting_id_1)).len());
        assert_eq!(1, context.get_setting_members(SettingCategory::School(school_setting_id_2)).len());
        assert_eq!(1, context.get_setting_members(SettingCategory::Home(home_setting_id_2)).len());
        assert_eq!(
            2,
            context.get_setting_members(SettingCategory::CensusTract(census_tract_setting_id)).len()
        );

        let p1 = context.query_result_iterator::<Person, _>((Age(43),)).next().unwrap();
        let itinerary_id_1 = context
            .query_result_iterator::<Itinerary, _>((BelongsTo(p1),))
            .next()
            .unwrap();
        assert_eq!(
            context.get_property::<Itinerary, ItineraryHomeId>(itinerary_id_1).0,
            home_setting_id_1
        );
        assert_eq!(
            context.get_property::<Itinerary, ItineraryCensusTractId>(itinerary_id_1).0,
            census_tract_setting_id
        );
        assert_eq!(
            context.get_property::<Itinerary, ItinerarySchoolId>(itinerary_id_1).0.unwrap(),
            school_setting_id_1
        );
        assert_eq!(
            context.get_property::<Itinerary, ItineraryWorkplaceId>(itinerary_id_1).0,
            None
        );
        assert!(context.get_property::<Itinerary, Activity>(itinerary_id_1).0);


    }

    #[test]
    fn check_synth_file_workplace() {
        let mut context = setup();

        let input = String::from(
            "age,homeId,schoolId,workplaceId\n43,360930331020001,,360930331020003\n42,360930331020002,,360930331020004",
        );
        let synth_file = persist_tmp_csv(&input);
        load_synth_population(&mut context, synth_file).unwrap();
        let age = [43, 42];
        let workplace_id = [360_930_331_020_003, 360_930_331_020_004];
        let home_id = [360_930_331_020_001, 360_930_331_020_002];
        let census_tract_id = 36_093_033_102;

        assert_eq!(context.get_entity_count::<Person>(), 2);
            assert_eq!(context.get_entity_count::<Home>(), 2);
            assert_eq!(context.get_entity_count::<CensusTract>(), 1);
            assert_eq!(context.get_entity_count::<Workplace>(), 2);

        let home_setting_id_1 = context
            .query_result_iterator::<Home, _>((
                SettingCode(home_id[0]),
            ))
            .next()
            .unwrap();
        let home_setting_id_2 = context
            .query_result_iterator::<Home, _>((
                SettingCode(home_id[1]),
            ))
            .next()
            .unwrap();
        let census_tract_setting_id = context
            .query_result_iterator::<CensusTract, _>((
                SettingCode(census_tract_id),
            ))
            .next()
            .unwrap();
        let workplace_setting_id_1 = context
            .query_result_iterator::<Workplace, _>((
                SettingCode(workplace_id[0]),
            ))
            .next()
            .unwrap();
        let workplace_setting_id_2 = context
            .query_result_iterator::<Workplace, _>((
                SettingCode(workplace_id[1]),
            ))
            .next()
            .unwrap();

        for item in age.iter().take(1) {
            assert_eq!(1, context.query_entity_count::<Person, _>((Age(*item),)));
        }
        assert_eq!(
            1,
            context.get_setting_members(SettingCategory::Workplace(workplace_setting_id_1)).len()
        );
        assert_eq!(1, context.get_setting_members(SettingCategory::Home(home_setting_id_1)).len());
        assert_eq!(
            1,
            context.get_setting_members(SettingCategory::Workplace(workplace_setting_id_2)).len()
        );
        assert_eq!(1, context.get_setting_members(SettingCategory::Home(home_setting_id_2)).len());
        assert_eq!(
            1,
            context.get_setting_members(SettingCategory::Workplace(workplace_setting_id_2)).len()
        );
        assert_eq!(
            2,
            context.get_setting_members(SettingCategory::CensusTract(census_tract_setting_id)).len()
        );
        let p1 = context.query_result_iterator::<Person, _>((Age(43),)).next().unwrap();
        let itinerary_id_1 = context
            .query_result_iterator::<Itinerary, _>((BelongsTo(p1),))
            .next()
            .unwrap();
        assert_eq!(
            context.get_property::<Itinerary, ItineraryHomeId>(itinerary_id_1).0,
            home_setting_id_1
        );
        assert_eq!(
            context.get_property::<Itinerary, ItineraryCensusTractId>(itinerary_id_1).0,
            census_tract_setting_id
        );
        assert_eq!(
            context.get_property::<Itinerary, ItinerarySchoolId>(itinerary_id_1).0,
            None
        );
        assert_eq!(
            context.get_property::<Itinerary, ItineraryWorkplaceId>(itinerary_id_1).0.unwrap(),
            workplace_setting_id_1
        );
        assert!(context.get_property::<Itinerary, Activity>(itinerary_id_1).0);
    }
}