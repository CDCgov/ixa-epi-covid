use ixa::{csv, prelude::*};

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::{itinerary::{BelongsTo, CensusTractItinerary, HomeItinerary, Itinerary, SchoolItinerary, WorkplaceItinerary}, parameters::{ContextParametersExt, Params}, setting_entities::{ContextSettingEntitiesExt, SettingCategory, SettingCode, SettingEntity, SettingEntityId}, settings::{CensusTract, ContextSettingExt, Home, School, SettingId, Workplace, append_itinerary_entry}};
// use crate::settings::{
//     CensusTract, ContextSettingExt, Home, School, SettingId, Workplace, append_itinerary_entry,
// };
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

#[allow(dead_code)]
fn create_person_from_record(
    context: &mut Context,
    person_record: &PeopleRecord,
) -> Result<(), IxaError> {
    // Create itinerary entries for all setting memberships in input file
    let tract: String = String::from_utf8(person_record.homeId[..11].to_owned())?;
    let home_id: String = String::from_utf8(person_record.homeId.to_owned())?;
    let school_string: String = String::from_utf8(person_record.schoolId.to_owned())?;
    let workplace_string: String = String::from_utf8(person_record.workplaceId.to_owned())?;

    // Add person to context
    let person_id: PersonId = context.add_entity((Age(person_record.age),)).unwrap();

    // Initialize a vector of home and census tract since everyone has these settings
    let mut itinerary = vec![];
    append_itinerary_entry(
        &mut itinerary,
        context,
        SettingId::new(Home, home_id.parse()?),
        None,
    )?;
    append_itinerary_entry(
        &mut itinerary,
        context,
        SettingId::new(CensusTract, tract.parse()?),
        None,
    )?;

    // Check for school and work memberships
    if !school_string.is_empty() {
        append_itinerary_entry(
            &mut itinerary,
            context,
            SettingId::new(School, school_string.parse()?),
            None,
        )?;
    }
    if !workplace_string.is_empty() {
        append_itinerary_entry(
            &mut itinerary,
            context,
            SettingId::new(Workplace, workplace_string.parse()?),
            None,
        )?;
    }

    // // Create the itinerary using write rules stored in Context
    context.add_itinerary(person_id, itinerary)?;

    Ok(())
}

#[allow(dead_code)]
fn create_person_from_record_base(
    context: &mut Context,
    person_record: &PeopleRecord,
) -> Result<(), IxaError> {

    // Add person to context
    let _person_id: PersonId = context.add_entity((Age(person_record.age),)).unwrap();

    Ok(())
}

#[allow(dead_code)]
fn create_person_with_one_setting(
    context: &mut Context,
    person_record: &PeopleRecord,
) -> Result<(), IxaError> {
    let home_id: String = String::from_utf8(person_record.homeId.to_owned())?;

    // Add person to context
    let person_id: PersonId = context.add_entity((Age(person_record.age),)).unwrap();

    // Initialize a vector of home and census tract since everyone has these settings
    let mut itinerary = vec![];
    append_itinerary_entry(
        &mut itinerary,
        context,
        SettingId::new(Home, home_id.parse()?),
        None,
    )?;
    context.add_itinerary(person_id, itinerary)?;
    Ok(())
}

#[allow(dead_code)]
fn create_person_with_two_setting(
    context: &mut Context,
    person_record: &PeopleRecord,
) -> Result<(), IxaError> {
    let tract: String = String::from_utf8(person_record.homeId[..11].to_owned())?;
    let home_id: String = String::from_utf8(person_record.homeId.to_owned())?;

    // Add person to context
    let person_id: PersonId = context.add_entity((Age(person_record.age),)).unwrap();

    // Initialize a vector of home and census tract since everyone has these settings
    let mut itinerary = vec![];
    append_itinerary_entry(
        &mut itinerary,
        context,
        SettingId::new(Home, home_id.parse()?),
        None,
    )?;

    append_itinerary_entry(
        &mut itinerary,
        context,
        SettingId::new(CensusTract, tract.parse()?),
        None,
    )?;
    context.add_itinerary(person_id, itinerary)?;
    Ok(())
}

#[allow(dead_code)]
fn create_person_from_record_with_setting_entities(
    context: &mut Context,
    person_record: &PeopleRecord,
) -> Result<(), IxaError> {

    // this assumes that you will not use the setting loader.

    // Create itinerary entries for all setting memberships in input file
    let tract: String = String::from_utf8(person_record.homeId[..11].to_owned())?;
    let home_id: String = String::from_utf8(person_record.homeId.to_owned())?;
    let school_string: String = String::from_utf8(person_record.schoolId.to_owned())?;
    let workplace_string: String = String::from_utf8(person_record.workplaceId.to_owned())?;

    // Add person to context
    let _person_id: PersonId = context.add_entity((Age(person_record.age),)).unwrap();

    let _home_setting_id: Option<SettingEntityId> =
        Some(context.add_setting(SettingCategory::Home, home_id.clone())?);
    let _tract_setting_id: Option<SettingEntityId> =
        Some(context.add_setting(SettingCategory::CensusTract, tract.clone())?);
    let _school_setting_id: Option<SettingEntityId> = if school_string.is_empty() {
        None
    } else {
        Some(context.add_setting(SettingCategory::School, school_string.clone())?)
    };
    let _workplace_setting_id: Option<SettingEntityId> = if workplace_string.is_empty() {
        None
    } else {
        Some(context.add_setting(SettingCategory::Workplace, workplace_string.clone())?)
    };

    Ok(())
}

#[allow(dead_code)]
fn create_person_from_record_with_setting_entities_and_itinerary(
    context: &mut Context,
    person_record: &PeopleRecord,
) -> Result<(), IxaError> {
    // this assumes that itinerary entities are implement and 
    // this assumes that the setting loader is used to load settings ids


    // Create itinerary entries for all setting memberships in input file
    let tract_id: Option<usize> = if person_record.homeId.len() >= 11 {
        Some(String::from_utf8(person_record.homeId[..11].to_owned())?.parse()?)
    } else {
        None
    };
    let home_id: Option<usize> = if !person_record.homeId.is_empty() {
        Some(String::from_utf8(person_record.homeId.to_owned())?.parse()?)
    } else {
        None
    };
    let school_id: Option<usize> = if !person_record.schoolId.is_empty() {
        Some(String::from_utf8(person_record.schoolId.to_owned())?.parse()?)
    } else {
        None
    };
    let workplace_id: Option<usize> = if !person_record.workplaceId.is_empty() {
        Some(String::from_utf8(person_record.workplaceId.to_owned())?.parse()?)
    } else {
        None
    };

    // Add person to context
    let person_id: PersonId = context.add_entity::<Person, _>((Age(person_record.age),)).unwrap();
    let home = if let Some(home_id) = home_id {
        context.query_result_iterator::<SettingEntity, _>((SettingCode(home_id),)).next()
    } else {
        None
    };
    let tract = if let Some(tract_id) = tract_id {
        context.query_result_iterator::<SettingEntity, _>((SettingCode(tract_id),)).next()
    } else {
        None
    };
    let school = if let Some(school_id) = school_id {
        context.query_result_iterator::<SettingEntity, _>((SettingCode(school_id),)).next()
    } else {
        None
    };
    let workplace = if let Some(workplace_id) = workplace_id {
        context.query_result_iterator::<SettingEntity, _>((SettingCode(workplace_id),)).next()
    } else {
        None
    };

    let home_itinerary = HomeItinerary {
        home_id: home,
        ratio: home.map(|_| 0.25),
    };

    let tract_itinerary = CensusTractItinerary {
        census_tract_id: tract,
        ratio: tract.map(|_| 0.25),
    };

    let school_itinerary = SchoolItinerary {
        school_id: school,
        ratio: school.map(|_| 0.25),
    };

    let workplace_itinerary = WorkplaceItinerary {
        workplace_id: workplace,
        ratio: workplace.map(|_| 0.25),
    };

    let _itinerary = context.add_entity::<Itinerary, _>((BelongsTo(person_id), 
    home_itinerary, tract_itinerary, school_itinerary, workplace_itinerary))?;
    
    Ok(())
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
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::parameters::{CoreSettingsTypes, GlobalParams};
    use crate::settings::{CensusTract, ContextSettingExt, Home, School, SettingId, SettingProperties, Workplace};
    use ixa::HashMap;
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
                    (CoreSettingsTypes::Home, SettingProperties { alpha: 0.0 }),
                    (CoreSettingsTypes::School, SettingProperties { alpha: 0.0 }),
                    (
                        CoreSettingsTypes::Workplace,
                        SettingProperties { alpha: 0.0 },
                    ),
                    (
                        CoreSettingsTypes::CensusTract,
                        SettingProperties { alpha: 0.0 },
                    ),
                ]
                .into_iter()
                .collect::<HashMap<_, _>>(),
            ),
            itinerary_ratios: HashMap::from_iter([
                (CoreSettingsTypes::Home, 0.25),
                (CoreSettingsTypes::School, 0.25),
                (CoreSettingsTypes::Workplace, 0.25),
                (CoreSettingsTypes::CensusTract, 0.25),
            ]),
            ..Default::default()
        };
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();
        crate::settings::init(&mut context);
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

        for i in 0..1 {
            assert_eq!(1, context.query_entity_count::<Person, _>((Age(age[i]),)));
            assert_eq!(
                1,
                context
                    .get_setting_members(&SettingId::new(Home, home_id[i]))
                    .unwrap()
                    .len()
            );
        }
        assert_eq!(
            2,
            context
                .get_setting_members(&SettingId::new(CensusTract, census_tract_id))
                .unwrap()
                .len()
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

        for i in 0..1 {
            assert_eq!(1, context.query_entity_count::<Person, _>((Age(age[i]),)));
            assert_eq!(
                1,
                context
                    .get_setting_members(&SettingId::new(School, school_id[i]))
                    .unwrap()
                    .len()
            );
            assert_eq!(
                1,
                context
                    .get_setting_members(&SettingId::new(Home, home_id[i]))
                    .unwrap()
                    .len()
            );
        }
        assert_eq!(
            2,
            context
                .get_setting_members(&SettingId::new(CensusTract, census_tract_id))
                .unwrap()
                .len()
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

        for i in 0..1 {
            assert_eq!(1, context.query_entity_count::<Person, _>((Age(age[i]),)));
            assert_eq!(
                1,
                context
                    .get_setting_members(&SettingId::new(Workplace, workplace_id[i]))
                    .unwrap()
                    .len()
            );
            assert_eq!(
                1,
                context
                    .get_setting_members(&SettingId::new(Home, home_id[i]))
                    .unwrap()
                    .len()
            );
        }
        assert_eq!(
            2,
            context
                .get_setting_members(&SettingId::new(CensusTract, census_tract_id))
                .unwrap()
                .len()
        );
    }
}
