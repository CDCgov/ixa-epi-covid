use ixa::{impl_derived_property, prelude::*};

use serde::Serialize;
use std::path::PathBuf;

use crate::error::ModelError;
use crate::parameters::ContextParametersExt;
use crate::pop_reader::{
    FIPSCode, PersonRecord, PopulationReaderSettingCategory,
    archive::{PersonRecordIterator, set_data_path},
};
use crate::settings::{ContextSettingExt, SETTING_COUNT, SettingCategory, SettingCode, SettingId};
use ixa::profiling::open_span;

define_entity!(Person);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct Age(pub u8);
impl_property!(Age, Person);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct Alive(pub bool);
impl_property!(Alive, Person, default_const = Alive(true));

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct SettingIds {
    pub setting_ids: [Option<SettingId>; SETTING_COUNT],
}
impl_property!(
    SettingIds,
    Person,
    default_const = SettingIds {
        setting_ids: [None; SETTING_COUNT]
    }
);

#[derive(Debug, PartialEq, Clone, Copy, Serialize)]
pub struct ItineraryRatios {
    pub itinerary_ratios: [f64; SETTING_COUNT],
}
impl_property!(
    ItineraryRatios,
    Person,
    default_const = ItineraryRatios {
        itinerary_ratios: [0.0; SETTING_COUNT]
    }
);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct HomeId(pub Option<SettingId>);
#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct SchoolId(pub Option<SettingId>);
#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct WorkId(pub Option<SettingId>);
#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct CommunityId(pub Option<SettingId>);

impl_derived_property!(HomeId, Person, [SettingIds], [], |setting_ids| HomeId(
    setting_ids.setting_ids[SettingCategory::Home]
));

impl_derived_property!(SchoolId, Person, [SettingIds], [], |setting_ids| SchoolId(
    setting_ids.setting_ids[SettingCategory::School]
));

impl_derived_property!(WorkId, Person, [SettingIds], [], |setting_ids| WorkId(
    setting_ids.setting_ids[SettingCategory::Work]
));

impl_derived_property!(CommunityId, Person, [SettingIds], [], |setting_ids| {
    CommunityId(setting_ids.setting_ids[SettingCategory::Community])
});

fn community_code_from_home(home_id: SettingCode) -> SettingCode {
    let home_id = home_id.0;
    // Since we are calling this constructor with values that we know are valid, we can unwrap.
    SettingCode(
        FIPSCode::with_category(
            home_id.state_code(),
            home_id.county_code(),
            home_id.census_tract_code(),
            PopulationReaderSettingCategory::CensusTract.encode(),
        )
        .unwrap(),
    )
}

fn create_person_from_record(
    context: &mut Context,
    person_record: PersonRecord,
) -> Result<(), ModelError> {
    let home_id = person_record.home_id.ok_or_else(|| {
        ModelError::ModelError("person record is missing required home_id".to_string())
    })?;
    let home_id = SettingCode(home_id);
    let community_id = community_code_from_home(home_id);

    let person_id: PersonId = context.add_entity((Age(person_record.age),)).unwrap();
    context.add_person_to_settings(
        person_id,
        Some(home_id),
        person_record.work_id.map(SettingCode),
        person_record.school_id.map(SettingCode),
        Some(community_id),
    )?;
    Ok(())
}

fn load_synth_population(
    context: &mut Context,
    synth_input_file: PathBuf,
) -> Result<(), ModelError> {
    set_data_path(PathBuf::from(env!("CARGO_MANIFEST_DIR")));

    let records = PersonRecordIterator::from_path(synth_input_file)?;
    for record in records {
        create_person_from_record(context, record?)?;
    }
    Ok(())
}

pub fn init(
    context: &mut Context,
    synth_population_override: Option<PathBuf>,
) -> Result<(), ModelError> {
    let _span = open_span("load_synth_population");
    let file = synth_population_override
        .unwrap_or_else(|| context.get_params().synth_population_file.clone());
    load_synth_population(context, file)?;
    context.index_property::<Person, HomeId>();
    context.index_property::<Person, SchoolId>();
    context.index_property::<Person, WorkId>();
    context.index_property::<Person, CommunityId>();
    context.initialize_setting_size()?;
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::parameters::{GlobalParams, Params, SettingProperties};
    use crate::pop_reader::{
        errors::PopulationReaderError,
        parser::{parse_fips_home_id, parse_fips_school_id, parse_fips_workplace_id},
    };
    use crate::settings::{Setting, SettingCategory, SettingCode};
    use ixa::HashMap;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;

    fn persist_tmp_csv(content: &str) -> PathBuf {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        let (_file, path) = file.keep().unwrap();
        path
    }

    fn make_home_id(home_id: &[u8]) -> SettingCode {
        SettingCode(parse_fips_home_id(home_id).unwrap().1)
    }

    fn make_school_id(school_id: &[u8]) -> SettingCode {
        SettingCode(parse_fips_school_id(school_id).unwrap().1)
    }

    fn make_workplace_id(workplace_id: &[u8]) -> SettingCode {
        SettingCode(parse_fips_workplace_id(workplace_id).unwrap().1)
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
        crate::settings::init(&mut context).unwrap();
        context
    }

    #[test]
    fn check_synth_file_tract() {
        let mut context = setup();
        let input = concat!(
            "age,homeId,schoolId,workplaceId\n",
            "43,160379602000001,,\n",
            "42,160379602000002,,\n",
        );
        let synth_file = persist_tmp_csv(input);
        load_synth_population(&mut context, synth_file).unwrap();
        context.initialize_setting_size().unwrap();
        let age = [43, 42];
        let home_id = [
            make_home_id(b"160379602000001"),
            make_home_id(b"160379602000002"),
        ];
        let census_tract_id = community_code_from_home(home_id[0]);

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
            .query_result_iterator::<Setting, _>((home_id[0], SettingCategory::Home))
            .next()
            .unwrap();
        let home_id2 = context
            .query_result_iterator::<Setting, _>((home_id[1], SettingCategory::Home))
            .next()
            .unwrap();
        let censustract_id = context
            .query_result_iterator::<Setting, _>((census_tract_id, SettingCategory::Community))
            .next()
            .unwrap();
        assert_eq!(1, context.get_setting_size(home_id1).unwrap());
        assert_eq!(1, context.get_setting_size(home_id2).unwrap());
        assert_eq!(2, context.get_setting_size(censustract_id).unwrap());
    }

    #[test]
    fn check_invalid_census_tract() {
        let mut context = setup();
        let input = concat!(
            "age,homeId,schoolId,workplaceId\n",
            "43,160379602,,\n",
            "42,160379602000002,,\n",
        );
        let synth_file = persist_tmp_csv(input);
        let error = load_synth_population(&mut context, synth_file).unwrap_err();

        assert!(matches!(
            error,
            ModelError::PopulationReaderError(PopulationReaderError::Parse {
                field_name: "homeId",
                line_number: 2,
                ..
            })
        ));
    }

    #[test]
    fn minimal_reproducible_test() {
        let mut context = setup();
        let home_code = make_home_id(b"160379602000001");
        let school_code = make_home_id(b"16037960200002");
        let record = PersonRecord {
            age: 43,
            home_id: Some(home_code.0),
            school_id: Some(school_code.0),
            work_id: None,
        };
        create_person_from_record(&mut context, record).unwrap_or_else(|e| panic!("{}", e));

        let home_id1 = context
            .query_result_iterator::<Setting, _>((home_code, SettingCategory::Home))
            .next()
            .unwrap();
        let cat: SettingCategory = context.get_property(home_id1);
        println!("Found {:?} for setting {:?}", cat, home_id1);
    }

    #[test]
    fn check_synth_file_school() {
        let mut context = setup();
        let input = concat!(
            "age,homeId,schoolId,workplaceId\n",
            "43,160379602000001,16037960200002,\n",
            "42,160379602000002,16037960200004,\n",
        );
        let synth_file = persist_tmp_csv(input);
        load_synth_population(&mut context, synth_file).unwrap_or_else(|e| panic!("{}", e));
        context.initialize_setting_size().unwrap();
        let age = [43, 42];
        let school_id = [
            make_school_id(b"16037960200002"),
            make_school_id(b"16037960200004"),
        ];
        let home_id = [
            make_home_id(b"160379602000001"),
            make_home_id(b"160379602000002"),
        ];
        let census_tract_id = community_code_from_home(home_id[0]);

        let home_id1 = context
            .query_result_iterator::<Setting, _>((home_id[0], SettingCategory::Home))
            .next()
            .unwrap();
        let cat: SettingCategory = context.get_property(home_id1);
        println!("Found {:?} for setting {:?}", cat, home_id1);
        let home_id2 = context
            .query_result_iterator::<Setting, _>((home_id[1], SettingCategory::Home))
            .next()
            .unwrap();
        let school_id1 = context
            .query_result_iterator::<Setting, _>((school_id[0], SettingCategory::School))
            .next()
            .unwrap();
        let school_id2 = context
            .query_result_iterator::<Setting, _>((school_id[1], SettingCategory::School))
            .next()
            .unwrap();
        let censustract_id = context
            .query_result_iterator::<Setting, _>((census_tract_id, SettingCategory::Community))
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
        let input = concat!(
            "age,homeId,schoolId,workplaceId\n",
            "43,160379602000001,,1603796020000220\n",
            "42,160379602000002,,1603796020001332\n",
        );
        let synth_file = persist_tmp_csv(input);
        load_synth_population(&mut context, synth_file).unwrap();
        context.initialize_setting_size().unwrap();

        let age = [43, 42];
        let workplace_id = [
            make_workplace_id(b"1603796020000220"),
            make_workplace_id(b"1603796020001332"),
        ];
        let home_id = [
            make_home_id(b"160379602000001"),
            make_home_id(b"160379602000002"),
        ];
        let census_tract_id = community_code_from_home(home_id[0]);

        let home_id1 = context
            .query_result_iterator::<Setting, _>((home_id[0], SettingCategory::Home))
            .next()
            .unwrap();
        let home_id2 = context
            .query_result_iterator::<Setting, _>((home_id[1], SettingCategory::Home))
            .next()
            .unwrap();
        let workplace_id1 = context
            .query_result_iterator::<Setting, _>((workplace_id[0], SettingCategory::Work))
            .next()
            .unwrap();
        let workplace_id2 = context
            .query_result_iterator::<Setting, _>((workplace_id[1], SettingCategory::Work))
            .next()
            .unwrap();
        let censustract_id = context
            .query_result_iterator::<Setting, _>((census_tract_id, SettingCategory::Community))
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
