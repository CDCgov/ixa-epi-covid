use ixa::{HashMap, csv, prelude::*};

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::parameters::{ContextParametersExt, Params};

use ixa::profiling::open_span;

define_entity!(Home);
define_entity!(Workplace);
define_entity!(School);
define_entity!(CensusTract);

// pub trait Setting: Entity {
//     type Id;
//     fn id(&self, context: &Context) -> Self::Id;
// }

// impl Setting for Home {
//     type Id = HomeId;
    
//     fn id(&self) -> HomeId {
        
//     }
// }

// impl Setting for Workplace {
//     type Id = WorkplaceId;
    
//     fn id(&self, context: &Context) -> WorkplaceId {
//         context.get_entity_id(*self)
//     }
// }

// impl Setting for School {
//     type Id = SchoolId;
    
//     fn id(&self, context: &Context) -> SchoolId {
//         context.get_entity_id(*self)
//     }
// }

// impl Setting for CensusTract {
//     type Id = CensusTractId;
    
//     fn id(&self, context: &Context) -> CensusTractId {
//         context.get_entity_id(*self)
//     }
// }

#[derive(Deserialize, Debug)]
#[allow(non_snake_case)]
pub struct SettingRecord<'a> {
    setting_category: &'a str,
    setting_code: &'a [u8],
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy)]
pub struct GeographyProperties {
    pub fips_code: usize,
}

impl_property!(GeographyProperties, Home);
impl_property!(GeographyProperties, Workplace);
impl_property!(GeographyProperties, School);
impl_property!(GeographyProperties, CensusTract);

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy)]
pub struct SettingEntityProperties {
    pub alpha: f64,
}

impl_property!(SettingEntityProperties, Home);
impl_property!(SettingEntityProperties, Workplace);
impl_property!(SettingEntityProperties, School);
impl_property!(SettingEntityProperties, CensusTract);

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy)]
pub struct DefaultItineraryProperties {
    pub ratio: f64,
}

impl_property!(DefaultItineraryProperties, Home);
impl_property!(DefaultItineraryProperties, Workplace);
impl_property!(DefaultItineraryProperties, School);
impl_property!(DefaultItineraryProperties, CensusTract);

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy, Eq, Hash)]
pub enum SettingCategory {
    Home(HomeId),
    Workplace(WorkplaceId),
    School(SchoolId),
    CensusTract(CensusTractId),
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Copy, Eq, Hash)]
pub enum DefaultSettingCategory {
    Home,
    Workplace,
    School,
    CensusTract,
}

fn create_setting_from_record(
    context: &mut Context,
    setting_record: &SettingRecord,
    setting_properties: HashMap<DefaultSettingCategory, SettingEntityProperties>,
    itinerary_ratios: HashMap<DefaultSettingCategory, f64>,
) -> Result<(), IxaError> {
    // Create itinerary entries for all setting memberships in input file
    let setting_category: String = setting_record.setting_category.to_string();
    let setting_code: String = String::from_utf8(setting_record.setting_code.to_owned())?;

    let fips_code: usize = setting_code.parse()?;
    let geography = GeographyProperties { fips_code };

    if setting_category == "homeId" {
        context.add_entity::<Home, _>((
            geography,
            setting_properties
                .get(&DefaultSettingCategory::Home)
                .copied()
                .unwrap_or(SettingEntityProperties { alpha: 0.0 }),
            DefaultItineraryProperties {
                ratio: itinerary_ratios
                    .get(&DefaultSettingCategory::Home)
                    .copied()
                    .unwrap_or(0.0),
            },
        ))?;
    } else if setting_category == "workplaceId" {
        context.add_entity::<Workplace, _>((
            geography,
            setting_properties
                .get(&DefaultSettingCategory::Workplace)
                .copied()
                .unwrap_or(SettingEntityProperties { alpha: 0.0 }),
            DefaultItineraryProperties {
                ratio: itinerary_ratios
                    .get(&DefaultSettingCategory::Workplace)
                    .copied()
                    .unwrap_or(0.0),
            },
        ))?;
    } else if setting_category == "schoolId" {
        context.add_entity::<School, _>((
            geography,
            setting_properties
                .get(&DefaultSettingCategory::School)
                .copied()
                .unwrap_or(SettingEntityProperties { alpha: 0.0 }),
            DefaultItineraryProperties {
                ratio: itinerary_ratios
                    .get(&DefaultSettingCategory::School)
                    .copied()
                    .unwrap_or(0.0),
            },
        ))?;
    } else if setting_category == "censustractId" {
        context.add_entity::<CensusTract, _>((
            geography,
            setting_properties
                .get(&DefaultSettingCategory::CensusTract)
                .copied()
                .unwrap_or(SettingEntityProperties { alpha: 0.0 }),
            DefaultItineraryProperties {
                ratio: itinerary_ratios
                    .get(&DefaultSettingCategory::CensusTract)
                    .copied()
                    .unwrap_or(0.0),
            },
        ))?;
    }
    Ok(())
}

pub fn load_settings(
    context: &mut Context,
    setting_file: PathBuf,
    setting_properties: HashMap<DefaultSettingCategory, SettingEntityProperties>,
    itinerary_ratios: HashMap<DefaultSettingCategory, f64>,
) -> Result<(), IxaError> {
    let mut reader = csv::Reader::from_path(setting_file)?;
    let mut raw_record = csv::ByteRecord::new();
    let headers = reader.byte_headers()?.clone();

    while reader.read_byte_record(&mut raw_record)? {
        let record: SettingRecord = raw_record.deserialize(Some(&headers))?;
        create_setting_from_record(
            context,
            &record,
            setting_properties.clone(),
            itinerary_ratios.clone(),
        )?;
    }
    Ok(())
}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    let _span = open_span("load_setting_population");
    let Params {
        setting_file,
        settings_properties,
        itinerary_ratios,
        ..
    } = context.get_params();
    load_settings(
        context,
        setting_file.clone(),
        settings_properties.clone(),
        itinerary_ratios.clone(),
    )?;
    Ok(())
}

// #[cfg(test)]
// mod test {
//     use super::*;
//     use crate::parameters::{CoreSettingsTypes, GlobalParams};
//     use crate::settings::{CensusTract, Home, School, SettingId, SettingProperties, Workplace};
//     use ixa::{ContextGlobalPropertiesExt, HashMap};
//     use std::io::Write;
//     use std::path::PathBuf;
//     use tempfile::NamedTempFile;

//     fn persist_tmp_csv(content: &String) -> PathBuf {
//         let mut file = NamedTempFile::new().unwrap();
//         file.write_all(content.as_bytes()).unwrap();
//         let (_file, path) = file.keep().unwrap();
//         path
//     }

//     fn setup() -> Context {
//         let mut context = Context::new();
//         let parameters = Params {
//             // We need to specify an itinerary split here even though we don't draw people from
//             // itineraries because `load_synth_population` calls `create_itinerary` for each person,
//             // and that function requires an itinerary write function to be set.
//             settings_properties: HashMap::from_iter(
//                 [
//                     (SettingCategory::Home, SettingProperties { alpha: 0.0 }),
//                     (SettingCategory::School, SettingProperties { alpha: 0.0 }),
//                     (
//                         SettingCategory::Workplace,
//                         SettingProperties { alpha: 0.0 },
//                     ),
//                     (
//                         SettingCategory::CensusTract,
//                         SettingProperties { alpha: 0.0 },
//                     ),
//                 ]
//                 .into_iter()
//                 .collect::<HashMap<_, _>>(),
//             ),
//             itinerary_ratios: HashMap::from_iter([
//                 (SettingCategory::Home, 0.25),
//                 (SettingCategory::School, 0.25),
//                 (SettingCategory::Workplace, 0.25),
//                 (SettingCategory::CensusTract, 0.25),
//             ]),
//             ..Default::default()
//         };
//         context
//             .set_global_property_value(GlobalParams, parameters)
//             .unwrap();
//         crate::settings::init(&mut context);
//         context
//     }

//     #[test]
//     fn check_synth_file_tract() {
//         let mut context = setup();
//         let input = String::from(
//             "age,homeId,schoolId,workplaceId\n43,360930331020001,,\n42,360930331020002,,",
//         );
//         let synth_file = persist_tmp_csv(&input);
//         load_synth_population(&mut context, synth_file).unwrap();
//         let age = [43, 42];
//         let home_id = [360_930_331_020_001, 360_930_331_020_002];
//         let census_tract_id = 36_093_033_102;

//         assert_eq!(context.get_entity_count::<Person>(), 2);

//         for i in 0..1 {
//             assert_eq!(1, context.query_entity_count::<Person, _>((Age(age[i]),)));
//             assert_eq!(
//                 1,
//                 context
//                     .get_setting_members(&SettingId::new(Home, home_id[i]))
//                     .unwrap()
//                     .len()
//             );
//         }
//         assert_eq!(
//             2,
//             context
//                 .get_setting_members(&SettingId::new(CensusTract, census_tract_id))
//                 .unwrap()
//                 .len()
//         );
//     }

//     #[test]
//     #[should_panic(expected = "range end index 11 out of range for slice of length 9")]
//     fn check_invalid_census_tract() {
//         let mut context = setup();
//         let input =
//             String::from("age,homeId,schoolId,workplaceId\n43,360930331,,\n42,360930331020002,,");
//         let synth_file = persist_tmp_csv(&input);
//         load_synth_population(&mut context, synth_file).unwrap();
//     }

//     #[test]
//     fn check_synth_file_school() {
//         let mut context = setup();
//         let input = String::from(
//             "age,homeId,schoolId,workplaceId\n43,360930331020001,1,\n42,360930331020002,2,",
//         );
//         let synth_file = persist_tmp_csv(&input);
//         load_synth_population(&mut context, synth_file).unwrap();
//         let age = [43, 42];
//         let school_id = [1, 2];
//         let home_id = [360_930_331_020_001, 360_930_331_020_002];
//         let census_tract_id = 36_093_033_102;

//         assert_eq!(context.get_entity_count::<Person>(), 2);

//         for i in 0..1 {
//             assert_eq!(1, context.query_entity_count::<Person, _>((Age(age[i]),)));
//             assert_eq!(
//                 1,
//                 context
//                     .get_setting_members(&SettingId::new(School, school_id[i]))
//                     .unwrap()
//                     .len()
//             );
//             assert_eq!(
//                 1,
//                 context
//                     .get_setting_members(&SettingId::new(Home, home_id[i]))
//                     .unwrap()
//                     .len()
//             );
//         }
//         assert_eq!(
//             2,
//             context
//                 .get_setting_members(&SettingId::new(CensusTract, census_tract_id))
//                 .unwrap()
//                 .len()
//         );
//     }

//     #[test]
//     fn check_synth_file_workplace() {
//         let mut context = setup();
//         let input = String::from(
//             "age,homeId,schoolId,workplaceId\n43,360930331020001,,1\n42,360930331020002,,2",
//         );
//         let synth_file = persist_tmp_csv(&input);
//         load_synth_population(&mut context, synth_file).unwrap();
//         let age = [43, 42];
//         let workplace_id = [1, 2];
//         let home_id = [360_930_331_020_001, 360_930_331_020_002];
//         let census_tract_id = 36_093_033_102;

//         assert_eq!(context.get_entity_count::<Person>(), 2);

//         for i in 0..1 {
//             assert_eq!(1, context.query_entity_count::<Person, _>((Age(age[i]),)));
//             assert_eq!(
//                 1,
//                 context
//                     .get_setting_members(&SettingId::new(Workplace, workplace_id[i]))
//                     .unwrap()
//                     .len()
//             );
//             assert_eq!(
//                 1,
//                 context
//                     .get_setting_members(&SettingId::new(Home, home_id[i]))
//                     .unwrap()
//                     .len()
//             );
//         }
//         assert_eq!(
//             2,
//             context
//                 .get_setting_members(&SettingId::new(CensusTract, census_tract_id))
//                 .unwrap()
//                 .len()
//         );
//     }
// }
