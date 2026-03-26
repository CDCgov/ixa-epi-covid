use ixa::{HashMap, csv, prelude::*};

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::ModelError;
use crate::parameters::ContextParametersExt;
use crate::settings::{
    Alpha, ContextSettingExt, SettingCategory, SettingCode, SettingId, SettingProperties,
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

impl_property!(HomeId, Person, default_const = HomeId(None));
impl_property!(WorkId, Person, default_const = WorkId(None));
impl_property!(SchoolId, Person, default_const = SchoolId(None));
impl_property!(CommunityId, Person, default_const = CommunityId(None));

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Hash)]
pub struct HomeId(pub Option<SettingId>);
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Hash)]
pub struct WorkId(pub Option<SettingId>);
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Hash)]
pub struct SchoolId(pub Option<SettingId>);
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Hash)]
pub struct CommunityId(pub Option<SettingId>);

pub trait Itinerary {
    fn get_setting_id(&self) -> Option<SettingId>;
}

impl Itinerary for HomeId {
    fn get_setting_id(&self) -> Option<SettingId> {
        self.0
    }
}
impl Itinerary for WorkId {
    fn get_setting_id(&self) -> Option<SettingId> {
        self.0
    }
}
impl Itinerary for SchoolId {
    fn get_setting_id(&self) -> Option<SettingId> {
        self.0
    }
}
impl Itinerary for CommunityId {
    fn get_setting_id(&self) -> Option<SettingId> {
        self.0
    }
}

fn create_person_from_record(
    context: &mut Context,
    person_record: &PeopleRecord,
) -> Result<(), ModelError> {
    settings_properties: &HashMap<SettingCategory, SettingProperties>,
) -> Result<(), IxaError> {
    // Create itinerary entries for all setting memberships in input file
    let tract: String = String::from_utf8(person_record.homeId[..11].to_owned())?;
    let home_id: String = String::from_utf8(person_record.homeId.to_owned())?;
    let school_string: String = String::from_utf8(person_record.schoolId.to_owned())?;
    let workplace_string: String = String::from_utf8(person_record.workplaceId.to_owned())?;

    // Add person to context
    let person_id: PersonId = context.add_entity((Age(person_record.age),)).unwrap();
    context.add_person_to_setting(
        person_id,
        SettingCategory::Home,
        SettingCode(home_id.parse()?),
        Alpha(
            settings_properties
                .get(&SettingCategory::Home)
                .unwrap()
                .alpha,
        ),
    )?;

    context.add_person_to_setting(
        person_id,
        SettingCategory::Community,
        SettingCode(tract.parse()?),
        Alpha(
            settings_properties
                .get(&SettingCategory::Community)
                .unwrap()
                .alpha,
        ),
    )?;

    if !school_string.is_empty() {
        context.add_person_to_setting(
            person_id,
            SettingCategory::School,
            SettingCode(school_string.parse()?),
            Alpha(
                settings_properties
                    .get(&SettingCategory::School)
                    .unwrap()
                    .alpha,
            ),
        )?;
    }

    if !workplace_string.is_empty() {
        context.add_person_to_setting(
            person_id,
            SettingCategory::Work,
            SettingCode(workplace_string.parse()?),
            Alpha(
                settings_properties
                    .get(&SettingCategory::Work)
                    .unwrap()
                    .alpha,
            ),
        )?;
    }

    Ok(())
}

fn load_synth_population(
    context: &mut Context,
    synth_input_file: PathBuf,
) -> Result<(), ModelError> {
    let mut reader = csv::Reader::from_path(synth_input_file)?;
    let mut raw_record = csv::ByteRecord::new();
    let headers = reader.byte_headers()?.clone();

    while reader.read_byte_record(&mut raw_record)? {
        let record: PeopleRecord = raw_record.deserialize(Some(&headers))?;
        create_person_from_record(context, &record, &settings_properties)?;
    }
    Ok(())
}

pub fn init(
    context: &mut Context,
    synth_population_override: Option<PathBuf>,
) -> Result<(), ModelError> {
    context.index_property::<Person, HomeId>();
    context.index_property::<Person, SchoolId>();
    context.index_property::<Person, WorkId>();
    context.index_property::<Person, CommunityId>();
    
    let _span = open_span("load_synth_population");    
    let file = synth_population_override
        .unwrap_or_else(|| context.get_params().synth_population_file.clone());
    load_synth_population(context, file)?;
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::parameters::GlobalParams, Params;
    use crate::settings::{Setting, SettingCategory};
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
                    (SettingCategory::Home, SettingProperties { alpha: 0.0 }),
                    (SettingCategory::School, SettingProperties { alpha: 0.0 }),
                    (SettingCategory::Work, SettingProperties { alpha: 0.0 }),
                    (SettingCategory::Community, SettingProperties { alpha: 0.0 }),
                ]
                .into_iter()
                .collect::<HashMap<_, _>>(),
            ),
            itinerary_ratios: HashMap::from_iter([
                (SettingCategory::Home, 0.25),
                (SettingCategory::School, 0.25),
                (SettingCategory::Work, 0.25),
                (SettingCategory::Community, 0.25),
            ]),
            ..Default::default()
        };
        context
            .set_global_property_value(GlobalParams, parameters)
            .unwrap();
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
        assert_eq!(
            context.query_entity_count::<Setting, _>((SettingCategory::Home,)),
            2
        );
        assert_eq!(
            context.query_entity_count::<Setting, _>((SettingCategory::Community,)),
            1
        );

        for item in age.iter().take(1) {
            assert_eq!(1, context.query_entity_count::<Person, _>((Age(*item),)));
        }
        let home_id1 = context
            .query_result_iterator::<Setting, _>((SettingCode(home_id[0]), SettingCategory::Home))
            .next()
            .unwrap();
        let home_id2 = context
            .query_result_iterator::<Setting, _>((SettingCode(home_id[1]), SettingCategory::Home))
            .next()
            .unwrap();
        let censustract_id = context
            .query_result_iterator::<Setting, _>((
                SettingCode(census_tract_id),
                SettingCategory::Community,
            ))
            .next()
            .unwrap();
        assert_eq!(1, context.get_setting_size(home_id1).unwrap());
        assert_eq!(1, context.get_setting_size(home_id2).unwrap());
        assert_eq!(2, context.get_setting_size(censustract_id).unwrap());
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

        let home_id1 = context
            .query_result_iterator::<Setting, _>((SettingCode(home_id[0]), SettingCategory::Home))
            .next()
            .unwrap();
        let home_id2 = context
            .query_result_iterator::<Setting, _>((SettingCode(home_id[1]), SettingCategory::Home))
            .next()
            .unwrap();
        let school_id1 = context
            .query_result_iterator::<Setting, _>((
                SettingCode(school_id[0]),
                SettingCategory::School,
            ))
            .next()
            .unwrap();
        let school_id2 = context
            .query_result_iterator::<Setting, _>((
                SettingCode(school_id[1]),
                SettingCategory::School,
            ))
            .next()
            .unwrap();
        let censustract_id = context
            .query_result_iterator::<Setting, _>((
                SettingCode(census_tract_id),
                SettingCategory::Community,
            ))
            .next()
            .unwrap();

        assert_eq!(context.get_entity_count::<Person>(), 2);
        assert_eq!(
            context.query_entity_count::<Setting, _>((SettingCategory::Home,)),
            2
        );
        assert_eq!(
            context.query_entity_count::<Setting, _>((SettingCategory::School,)),
            2
        );
        assert_eq!(
            context.query_entity_count::<Setting, _>((SettingCategory::Community,)),
            1
        );

        for item in age.iter().take(1) {
            assert_eq!(1, context.query_entity_count::<Person, _>((Age(*item),)));
        }
        assert_eq!(1, context.get_setting_size(home_id1).unwrap());
        assert_eq!(1, context.get_setting_size(home_id2).unwrap());
        assert_eq!(1, context.get_setting_size(school_id1).unwrap());
        assert_eq!(1, context.get_setting_size(school_id2).unwrap());
        assert_eq!(2, context.get_setting_size(censustract_id).unwrap());
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

        let home_id1 = context
            .query_result_iterator::<Setting, _>((SettingCode(home_id[0]), SettingCategory::Home))
            .next()
            .unwrap();
        let home_id2 = context
            .query_result_iterator::<Setting, _>((SettingCode(home_id[1]), SettingCategory::Home))
            .next()
            .unwrap();
        let workplace_id1 = context
            .query_result_iterator::<Setting, _>((
                SettingCode(workplace_id[0]),
                SettingCategory::Work,
            ))
            .next()
            .unwrap();
        let workplace_id2 = context
            .query_result_iterator::<Setting, _>((
                SettingCode(workplace_id[1]),
                SettingCategory::Work,
            ))
            .next()
            .unwrap();
        let censustract_id = context
            .query_result_iterator::<Setting, _>((
                SettingCode(census_tract_id),
                SettingCategory::Community,
            ))
            .next()
            .unwrap();

        assert_eq!(context.get_entity_count::<Person>(), 2);
        assert_eq!(
            context.query_entity_count::<Setting, _>((SettingCategory::Home,)),
            2
        );
        assert_eq!(
            context.query_entity_count::<Setting, _>((SettingCategory::Work,)),
            2
        );
        assert_eq!(
            context.query_entity_count::<Setting, _>((SettingCategory::Community,)),
            1
        );
        for item in age.iter().take(1) {
            assert_eq!(1, context.query_entity_count::<Person, _>((Age(*item),)));
        }
        assert_eq!(1, context.get_setting_size(home_id1).unwrap());
        assert_eq!(1, context.get_setting_size(home_id2).unwrap());
        assert_eq!(1, context.get_setting_size(workplace_id1).unwrap());
        assert_eq!(1, context.get_setting_size(workplace_id2).unwrap());
        assert_eq!(2, context.get_setting_size(censustract_id).unwrap());
    }
}
