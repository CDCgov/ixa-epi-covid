use ixa::{impl_derived_property, prelude::*, profiling::open_span};

use serde::Serialize;
use std::path::PathBuf;

use crate::error::ModelError;
use crate::parameters::ContextParametersExt;
use crate::pop_reader::{
    PersonRecord,
    archive::{PersonRecordIterator, set_data_path},
};
use crate::setting_code::SettingCode;
use crate::settings::{ContextSettingExt, SETTING_COUNT, SettingCategory};

define_entity!(Person);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct Age(pub u8);
impl_property!(Age, Person);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct Alive(pub bool);
impl_property!(Alive, Person, default_const = Alive(true));

#[derive(Debug, PartialEq, Clone, Copy, Serialize)]
pub struct Itinerary {
    pub setting_ids: [Option<SettingCode>; SETTING_COUNT],
    pub itinerary_ratios: [f64; SETTING_COUNT],
}
impl_property!(
    Itinerary,
    Person,
    default_const = Itinerary {
        setting_ids: [None; SETTING_COUNT],
        itinerary_ratios: [0.0; SETTING_COUNT]
    }
);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct HomeId(pub Option<SettingCode>);
#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct SchoolId(pub Option<SettingCode>);
#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct WorkId(pub Option<SettingCode>);
#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Hash)]
pub struct CommunityId(pub Option<SettingCode>);

impl_derived_property!(HomeId, Person, [Itinerary], [], |itinerary| HomeId(
    itinerary.setting_ids[SettingCategory::Home]
));

impl_derived_property!(SchoolId, Person, [Itinerary], [], |itinerary| SchoolId(
    itinerary.setting_ids[SettingCategory::School]
));

impl_derived_property!(WorkId, Person, [Itinerary], [], |itinerary| WorkId(
    itinerary.setting_ids[SettingCategory::Work]
));

impl_derived_property!(CommunityId, Person, [Itinerary], [], |itinerary| {
    CommunityId(itinerary.setting_ids[SettingCategory::Community])
});

fn create_person_from_record(
    context: &mut Context,
    person_record: PersonRecord,
) -> Result<(), ModelError> {
    let home_id = person_record.home_id.ok_or_else(|| {
        ModelError::ModelError("person record is missing required home_id".to_string())
    })?;
    let home_id = SettingCode(home_id);
    let community_id = home_id.extract_community();

    let person_id: PersonId = context.add_entity((Age(person_record.age),)).unwrap();
    context.add_person_to_settings(
        person_id,
        Some(home_id),
        person_record.work_id.map(SettingCode),
        person_record.school_id.map(SettingCode),
        Some(community_id),
    );
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
    use crate::setting_code::SettingCode;
    use crate::settings::SettingCategory;
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

        let age = [43, 42];
        let home_id = [
            make_home_id(b"160379602000001"),
            make_home_id(b"160379602000002"),
        ];
        let census_tract_id = home_id[0].extract_community();

        assert_eq!(context.get_entity_count::<Person>(), 2);

        for item in age.iter().take(1) {
            assert_eq!(1, context.query_entity_count::<Person, _>((Age(*item),)));
        }
        let home_id1 = home_id[0];
        let home_id2 = home_id[1];

        assert_eq!(1, context.get_setting_size(home_id1));
        assert_eq!(1, context.get_setting_size(home_id2));
        assert_eq!(2, context.get_setting_size(census_tract_id));
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
    fn check_synth_file_school() {
        let mut context = setup();
        let input = concat!(
            "age,homeId,schoolId,workplaceId\n",
            "43,160379602000001,16037960200002,\n",
            "42,160379602000002,16037960200004,\n",
        );
        let synth_file = persist_tmp_csv(input);
        load_synth_population(&mut context, synth_file).unwrap_or_else(|e| panic!("{}", e));
        let age = [43, 42];
        let school_id = [
            make_school_id(b"16037960200002"),
            make_school_id(b"16037960200004"),
        ];
        let home_id = [
            make_home_id(b"160379602000001"),
            make_home_id(b"160379602000002"),
        ];
        let census_tract_id = home_id[0].extract_community();

        assert_eq!(context.get_entity_count::<Person>(), 2);

        for item in age.iter().take(1) {
            assert_eq!(1, context.query_entity_count::<Person, _>((Age(*item),)));
        }
        assert_eq!(1, context.get_setting_size(home_id[0]));
        assert_eq!(1, context.get_setting_size(home_id[1]));
        assert_eq!(1, context.get_setting_size(school_id[0]));
        assert_eq!(1, context.get_setting_size(school_id[1]));
        assert_eq!(2, context.get_setting_size(census_tract_id));
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

        let age = [43, 42];
        let workplace_id = [
            make_workplace_id(b"1603796020000220"),
            make_workplace_id(b"1603796020001332"),
        ];
        let home_id = [
            make_home_id(b"160379602000001"),
            make_home_id(b"160379602000002"),
        ];
        let census_tract_id = home_id[0].extract_community();

        assert_eq!(context.get_entity_count::<Person>(), 2);

        for item in age.iter().take(1) {
            assert_eq!(1, context.query_entity_count::<Person, _>((Age(*item),)));
        }
        assert_eq!(1, context.get_setting_size(home_id[0]));
        assert_eq!(1, context.get_setting_size(home_id[1]));
        assert_eq!(1, context.get_setting_size(workplace_id[0]));
        assert_eq!(1, context.get_setting_size(workplace_id[1]));
        assert_eq!(2, context.get_setting_size(census_tract_id));
    }
}
