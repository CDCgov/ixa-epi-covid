use ixa::{csv, prelude::*};

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

<<<<<<< HEAD
use crate::parameters::{ContextParametersExt, Params};
use crate::settings::{
    CensusTract, ContextSettingExt, Home, School, SettingId, Workplace, append_itinerary_entry,
};
=======
use crate::{
    itinerary::{
        Activity, BelongsTo, CensusTractId, CensusTractItinerary, ContextItineraryExt, HomeId,
        HomeItinerary, Itinerary, SchoolId, SchoolItinerary, WorkplaceId, WorkplaceItinerary,
    },
    parameters::{ContextParametersExt, Params},
    settings_entities::{ContextSettingExt, Setting, SettingCategory, SettingCode, SettingId},
};

>>>>>>> 8ec06b6 (Squash)
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
pub enum IsolationStatus {
    Isolated,
    NotIsolated,
}
impl_property!(IsolationStatus, Person, default_const = IsolationStatus::NotIsolated);

fn create_person_from_record(
    context: &mut Context,
    person_record: &PeopleRecord,
) -> Result<(), IxaError> {
    // Create itinerary entries for all setting memberships in input file
    let tract: String = String::from_utf8(person_record.homeId[..11].to_owned())?;
    let home_id: String = String::from_utf8(person_record.homeId.to_owned())?;
    let school_string: String = String::from_utf8(person_record.schoolId.to_owned())?;
    let workplace_string: String = String::from_utf8(person_record.workplaceId.to_owned())?;

    let home_setting_id: Option<SettingId> =
        Some(context.add_setting(SettingCategory::Home, home_id.clone())?);
    let tract_setting_id: Option<SettingId> =
        Some(context.add_setting(SettingCategory::CensusTract, tract.clone())?);
    let school_setting_id: Option<SettingId> = if school_string.is_empty() {
        None
    } else {
        Some(context.add_setting(SettingCategory::School, school_string.clone())?)
    };
    let workplace_setting_id: Option<SettingId> = if workplace_string.is_empty() {
        None
    } else {
        Some(context.add_setting(SettingCategory::Workplace, workplace_string.clone())?)
    };

    // Add person to context
    let person_id: PersonId = context.add_entity((Age(person_record.age),)).unwrap();

    let _itinerary_id = context
        .add_entity((
            BelongsTo(person_id),
            HomeItinerary {
                home_id: home_setting_id,
                ratio: Some(context.get_default_itinerary_ratio(SettingCategory::Home)?),
            },
            SchoolItinerary {
                school_id: school_setting_id,
                ratio: Some(context.get_default_itinerary_ratio(SettingCategory::School)?),
            },
            WorkplaceItinerary {
                workplace_id: workplace_setting_id,
                ratio: Some(context.get_default_itinerary_ratio(SettingCategory::Workplace)?),
            },
            CensusTractItinerary {
                census_tract_id: tract_setting_id,
                ratio: Some(context.get_default_itinerary_ratio(SettingCategory::CensusTract)?),
            },
        ))
        .unwrap();
    context.normalize_itinerary_ratios(person_id);

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
    context.index_property::<Itinerary, HomeId>();
    context.index_property::<Itinerary, SchoolId>();
    context.index_property::<Itinerary, WorkplaceId>();
    context.index_property::<Itinerary, CensusTractId>();
    context.index_property::<Itinerary, (BelongsTo, Activity)>();
    context.index_property::<Itinerary, BelongsTo>();
    context.index_property::<Setting, SettingCode>();

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::itinerary::ContextItineraryExt;
    use crate::parameters::GlobalParams;
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
                    (SettingCategory::Home, 0.0),
                    (SettingCategory::School, 0.0),
                    (SettingCategory::Workplace, 0.0),
                    (SettingCategory::CensusTract, 0.0),
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
        assert_eq!(context.get_entity_count::<Itinerary>(), 2);

        for item in age.iter().take(1) {
            assert_eq!(1, context.query_entity_count::<Person, _>((Age(*item),)));
        }

        let home_setting_id_1 = context
            .query_result_iterator::<Setting, _>((SettingCode(home_id[0]), SettingCategory::Home))
            .next()
            .unwrap();
        let home_setting_id_2 = context
            .query_result_iterator::<Setting, _>((SettingCode(home_id[1]), SettingCategory::Home))
            .next()
            .unwrap();
        let census_tract_setting_id = context
            .query_result_iterator::<Setting, _>((
                SettingCode(census_tract_id),
                SettingCategory::CensusTract,
            ))
            .next()
            .unwrap();

        assert_eq!(1, context.get_setting_members(home_setting_id_1).len());

        assert_eq!(1, context.get_setting_members(home_setting_id_2).len());

        assert_eq!(
            2,
            context.get_setting_members(census_tract_setting_id).len()
        );

        let p1 = context
            .query_result_iterator::<Person, _>((Age(43),))
            .next()
            .unwrap();
        let itinerary_id_1 = context
            .query_result_iterator::<Itinerary, _>((BelongsTo(p1),))
            .next()
            .unwrap();
        assert_eq!(
            context
                .get_property::<Itinerary, HomeId>(itinerary_id_1)
                .0
                .unwrap(),
            home_setting_id_1
        );
        assert_eq!(
            context
                .get_property::<Itinerary, CensusTractId>(itinerary_id_1)
                .0
                .unwrap(),
            census_tract_setting_id
        );
        assert_eq!(
            context
                .get_property::<Itinerary, SchoolId>(itinerary_id_1)
                .0,
            None
        );
        assert_eq!(
            context
                .get_property::<Itinerary, WorkplaceId>(itinerary_id_1)
                .0,
            None
        );
        assert!(
            context
                .get_property::<Itinerary, Activity>(itinerary_id_1)
                .0
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
            "age,homeId,schoolId,workplaceId\n43,360930331020001,360930331020003,\n42,360930331020002,360930331020004,",
        );
        let synth_file = persist_tmp_csv(&input);
        load_synth_population(&mut context, synth_file).unwrap();
        let age = [43, 42];
        let school_id = [360_930_331_020_003, 360_930_331_020_004];
        let home_id = [360_930_331_020_001, 360_930_331_020_002];
        let census_tract_id = 36_093_033_102;

        assert_eq!(context.get_entity_count::<Person>(), 2);
        assert_eq!(context.get_entity_count::<Setting>(), 5);

        let home_setting_id_1 = context
            .query_result_iterator::<Setting, _>((SettingCode(home_id[0]), SettingCategory::Home))
            .next()
            .unwrap();
        let home_setting_id_2 = context
            .query_result_iterator::<Setting, _>((SettingCode(home_id[1]), SettingCategory::Home))
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
        assert_eq!(1, context.get_setting_members(school_setting_id_1).len());
        assert_eq!(1, context.get_setting_members(home_setting_id_1).len());
        assert_eq!(1, context.get_setting_members(school_setting_id_2).len());
        assert_eq!(1, context.get_setting_members(home_setting_id_2).len());
        assert_eq!(
            2,
            context.get_setting_members(census_tract_setting_id).len()
        );

        let p1 = context
            .query_result_iterator::<Person, _>((Age(43),))
            .next()
            .unwrap();
        let itinerary_id_1 = context
            .query_result_iterator::<Itinerary, _>((BelongsTo(p1),))
            .next()
            .unwrap();
        assert_eq!(
            context
                .get_property::<Itinerary, HomeId>(itinerary_id_1)
                .0
                .unwrap(),
            home_setting_id_1
        );
        assert_eq!(
            context
                .get_property::<Itinerary, CensusTractId>(itinerary_id_1)
                .0
                .unwrap(),
            census_tract_setting_id
        );
        assert_eq!(
            context
                .get_property::<Itinerary, SchoolId>(itinerary_id_1)
                .0
                .unwrap(),
            school_setting_id_1
        );
        assert_eq!(
            context
                .get_property::<Itinerary, WorkplaceId>(itinerary_id_1)
                .0,
            None
        );
        assert!(
            context
                .get_property::<Itinerary, Activity>(itinerary_id_1)
                .0
        );
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
        assert_eq!(context.get_entity_count::<Setting>(), 5);

        let home_setting_id_1 = context
            .query_result_iterator::<Setting, _>((SettingCode(home_id[0]), SettingCategory::Home))
            .next()
            .unwrap();
        let home_setting_id_2 = context
            .query_result_iterator::<Setting, _>((SettingCode(home_id[1]), SettingCategory::Home))
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
        assert_eq!(1, context.get_setting_members(workplace_setting_id_1).len());
        assert_eq!(1, context.get_setting_members(home_setting_id_1).len());
        assert_eq!(1, context.get_setting_members(workplace_setting_id_2).len());
        assert_eq!(1, context.get_setting_members(home_setting_id_2).len());
        assert_eq!(1, context.get_setting_members(workplace_setting_id_2).len());
        assert_eq!(
            2,
            context.get_setting_members(census_tract_setting_id).len()
        );
        let p1 = context
            .query_result_iterator::<Person, _>((Age(43),))
            .next()
            .unwrap();
        let itinerary_id_1 = context
            .query_result_iterator::<Itinerary, _>((BelongsTo(p1),))
            .next()
            .unwrap();
        assert_eq!(
            context
                .get_property::<Itinerary, HomeId>(itinerary_id_1)
                .0
                .unwrap(),
            home_setting_id_1
        );
        assert_eq!(
            context
                .get_property::<Itinerary, CensusTractId>(itinerary_id_1)
                .0
                .unwrap(),
            census_tract_setting_id
        );
        assert_eq!(
            context
                .get_property::<Itinerary, SchoolId>(itinerary_id_1)
                .0,
            None
        );
        assert_eq!(
            context
                .get_property::<Itinerary, WorkplaceId>(itinerary_id_1)
                .0
                .unwrap(),
            workplace_setting_id_1
        );
        assert!(
            context
                .get_property::<Itinerary, Activity>(itinerary_id_1)
                .0
        );
    }
}
