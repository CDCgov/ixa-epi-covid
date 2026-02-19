// use ixa::{HashMap, csv, prelude::*};

// use serde::{Deserialize, Serialize};
// use std::path::PathBuf;

// use crate::parameters::{ContextParametersExt, Params};

// use ixa::profiling::open_span;

// fn create_setting_from_record(
//     context: &mut Context,
//     setting_record: &SettingRecord,
//     setting_properties: HashMap<SettingCategory, SettingEntityProperties>,
//     itinerary_ratios: HashMap<SettingCategory, f64>,
// ) -> Result<(), IxaError> {
//     // Create itinerary entries for all setting memberships in input file
//     let setting_category: String = setting_record.setting_category.to_string();
//     let setting_code: String = String::from_utf8(setting_record.setting_code.to_owned())?;

//     let fips_code: usize = setting_code.parse()?;
//     let geography = GeographyProperties { fips_code };

//     let _setting_id = match setting_category.as_str() {
//         "homeId" => context.add_entity((
//             SettingCategory::Home,
//             geography,
//             setting_properties
//                 .get(&SettingCategory::Home)
//                 .copied()
//                 .unwrap_or(SettingEntityProperties { alpha: 0.0 }),
//             DefaultItineraryProperties {
//                 ratio: itinerary_ratios
//                     .get(&SettingCategory::Home)
//                     .copied()
//                     .unwrap_or(0.0),
//             },
//         ))?,
//         "workplaceId" => context.add_entity((
//             SettingCategory::Workplace,
//             geography,
//             setting_properties
//                 .get(&SettingCategory::Workplace)
//                 .copied()
//                 .unwrap_or(SettingEntityProperties { alpha: 0.0 }),
//             DefaultItineraryProperties {
//                 ratio: itinerary_ratios
//                     .get(&SettingCategory::Workplace)
//                     .copied()
//                     .unwrap_or(0.0),
//             },
//         ))?,
//         "schoolId" => context.add_entity((
//             SettingCategory::School,
//             geography,
//             setting_properties
//                 .get(&SettingCategory::School)
//                 .copied()
//                 .unwrap_or(SettingEntityProperties { alpha: 0.0 }),
//             DefaultItineraryProperties {
//                 ratio: itinerary_ratios
//                     .get(&SettingCategory::School)
//                     .copied()
//                     .unwrap_or(0.0),
//             },
//         ))?,
//         "censustractId" => context.add_entity((
//             SettingCategory::CensusTract,
//             geography,
//             setting_properties
//                 .get(&SettingCategory::CensusTract)
//                 .copied()
//                 .unwrap_or(SettingEntityProperties { alpha: 0.0 }),
//             DefaultItineraryProperties {
//                 ratio: itinerary_ratios
//                     .get(&SettingCategory::CensusTract)
//                     .copied()
//                     .unwrap_or(0.0),
//             },
//         ))?,
//         _ => {
//             return Err(IxaError::IxaError(format!(
//                 "Invalid setting category {} in settings file",
//                 setting_category
//             )));
//         }
//     };

//     Ok(())
// }

// pub fn load_settings(
//     context: &mut Context,
//     setting_file: PathBuf,
//     setting_properties: HashMap<SettingCategory, SettingEntityProperties>,
//     itinerary_ratios: HashMap<SettingCategory, f64>,
// ) -> Result<(), IxaError> {
//     let mut reader = csv::Reader::from_path(setting_file)?;
//     let mut raw_record = csv::ByteRecord::new();
//     let headers = reader.byte_headers()?.clone();

//     while reader.read_byte_record(&mut raw_record)? {
//         let record: SettingRecord = raw_record.deserialize(Some(&headers))?;
//         create_setting_from_record(
//             context,
//             &record,
//             setting_properties.clone(),
//             itinerary_ratios.clone(),
//         )?;
//     }
//     Ok(())
// }

// pub fn init(context: &mut Context) -> Result<(), IxaError> {
//     let _span = open_span("load_setting_population");
//     let Params {
//         setting_file,
//         settings_properties,
//         itinerary_ratios,
//         ..
//     } = context.get_params();
//     load_settings(
//         context,
//         setting_file.clone(),
//         settings_properties.clone(),
//         itinerary_ratios.clone(),
//     )?;
//     Ok(())
// }

// // #[cfg(test)]
// // mod test {
// //     use super::*;
// //     use crate::parameters::{CoreSettingsTypes, GlobalParams};
// //     use crate::settings::{CensusTract, Home, School, SettingId, SettingProperties, Workplace};
// //     use ixa::{ContextGlobalPropertiesExt, HashMap};
// //     use std::io::Write;
// //     use std::path::PathBuf;
// //     use tempfile::NamedTempFile;

// //     fn persist_tmp_csv(content: &String) -> PathBuf {
// //         let mut file = NamedTempFile::new().unwrap();
// //         file.write_all(content.as_bytes()).unwrap();
// //         let (_file, path) = file.keep().unwrap();
// //         path
// //     }

// //     fn setup() -> Context {
// //         let mut context = Context::new();
// //         let parameters = Params {
// //             // We need to specify an itinerary split here even though we don't draw people from
// //             // itineraries because `load_synth_population` calls `create_itinerary` for each person,
// //             // and that function requires an itinerary write function to be set.
// //             settings_properties: HashMap::from_iter(
// //                 [
// //                     (SettingCategory::Home, SettingProperties { alpha: 0.0 }),
// //                     (SettingCategory::School, SettingProperties { alpha: 0.0 }),
// //                     (
// //                         SettingCategory::Workplace,
// //                         SettingProperties { alpha: 0.0 },
// //                     ),
// //                     (
// //                         SettingCategory::CensusTract,
// //                         SettingProperties { alpha: 0.0 },
// //                     ),
// //                 ]
// //                 .into_iter()
// //                 .collect::<HashMap<_, _>>(),
// //             ),
// //             itinerary_ratios: HashMap::from_iter([
// //                 (SettingCategory::Home, 0.25),
// //                 (SettingCategory::School, 0.25),
// //                 (SettingCategory::Workplace, 0.25),
// //                 (SettingCategory::CensusTract, 0.25),
// //             ]),
// //             ..Default::default()
// //         };
// //         context
// //             .set_global_property_value(GlobalParams, parameters)
// //             .unwrap();
// //         crate::settings::init(&mut context);
// //         context
// //     }

// //     #[test]
// //     fn check_synth_file_tract() {
// //         let mut context = setup();
// //         let input = String::from(
// //             "age,homeId,schoolId,workplaceId\n43,360930331020001,,\n42,360930331020002,,",
// //         );
// //         let synth_file = persist_tmp_csv(&input);
// //         load_synth_population(&mut context, synth_file).unwrap();
// //         let age = [43, 42];
// //         let home_id = [360_930_331_020_001, 360_930_331_020_002];
// //         let census_tract_id = 36_093_033_102;

// //         assert_eq!(context.get_entity_count::<Person>(), 2);

// //         for i in 0..1 {
// //             assert_eq!(1, context.query_entity_count::<Person, _>((Age(age[i]),)));
// //             assert_eq!(
// //                 1,
// //                 context
// //                     .get_setting_members(&SettingId::new(Home, home_id[i]))
// //                     .unwrap()
// //                     .len()
// //             );
// //         }
// //         assert_eq!(
// //             2,
// //             context
// //                 .get_setting_members(&SettingId::new(CensusTract, census_tract_id))
// //                 .unwrap()
// //                 .len()
// //         );
// //     }

// //     #[test]
// //     #[should_panic(expected = "range end index 11 out of range for slice of length 9")]
// //     fn check_invalid_census_tract() {
// //         let mut context = setup();
// //         let input =
// //             String::from("age,homeId,schoolId,workplaceId\n43,360930331,,\n42,360930331020002,,");
// //         let synth_file = persist_tmp_csv(&input);
// //         load_synth_population(&mut context, synth_file).unwrap();
// //     }

// //     #[test]
// //     fn check_synth_file_school() {
// //         let mut context = setup();
// //         let input = String::from(
// //             "age,homeId,schoolId,workplaceId\n43,360930331020001,1,\n42,360930331020002,2,",
// //         );
// //         let synth_file = persist_tmp_csv(&input);
// //         load_synth_population(&mut context, synth_file).unwrap();
// //         let age = [43, 42];
// //         let school_id = [1, 2];
// //         let home_id = [360_930_331_020_001, 360_930_331_020_002];
// //         let census_tract_id = 36_093_033_102;

// //         assert_eq!(context.get_entity_count::<Person>(), 2);

// //         for i in 0..1 {
// //             assert_eq!(1, context.query_entity_count::<Person, _>((Age(age[i]),)));
// //             assert_eq!(
// //                 1,
// //                 context
// //                     .get_setting_members(&SettingId::new(School, school_id[i]))
// //                     .unwrap()
// //                     .len()
// //             );
// //             assert_eq!(
// //                 1,
// //                 context
// //                     .get_setting_members(&SettingId::new(Home, home_id[i]))
// //                     .unwrap()
// //                     .len()
// //             );
// //         }
// //         assert_eq!(
// //             2,
// //             context
// //                 .get_setting_members(&SettingId::new(CensusTract, census_tract_id))
// //                 .unwrap()
// //                 .len()
// //         );
// //     }

// //     #[test]
// //     fn check_synth_file_workplace() {
// //         let mut context = setup();
// //         let input = String::from(
// //             "age,homeId,schoolId,workplaceId\n43,360930331020001,,1\n42,360930331020002,,2",
// //         );
// //         let synth_file = persist_tmp_csv(&input);
// //         load_synth_population(&mut context, synth_file).unwrap();
// //         let age = [43, 42];
// //         let workplace_id = [1, 2];
// //         let home_id = [360_930_331_020_001, 360_930_331_020_002];
// //         let census_tract_id = 36_093_033_102;

// //         assert_eq!(context.get_entity_count::<Person>(), 2);

// //         for i in 0..1 {
// //             assert_eq!(1, context.query_entity_count::<Person, _>((Age(age[i]),)));
// //             assert_eq!(
// //                 1,
// //                 context
// //                     .get_setting_members(&SettingId::new(Workplace, workplace_id[i]))
// //                     .unwrap()
// //                     .len()
// //             );
// //             assert_eq!(
// //                 1,
// //                 context
// //                     .get_setting_members(&SettingId::new(Home, home_id[i]))
// //                     .unwrap()
// //                     .len()
// //             );
// //         }
// //         assert_eq!(
// //             2,
// //             context
// //                 .get_setting_members(&SettingId::new(CensusTract, census_tract_id))
// //                 .unwrap()
// //                 .len()
// //         );
// //     }
// // }
