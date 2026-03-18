use ixa::{csv, prelude::*};

use serde::{Deserialize};
use std::path::PathBuf;

use crate::{parameters::{ContextParametersExt, Params}, setting_entities::{Alpha, DefaultItineraryRatio, SettingEntity, SettingCategory, SettingCode}};

use ixa::profiling::open_span;

#[derive(Deserialize, Debug)]
#[allow(non_snake_case)]
pub struct SettingRecord<'a> {
    settingcategory: &'a str,
    settingcode: &'a [u8],
}

fn create_setting_from_record(
    context: &mut Context,
    setting_record: &SettingRecord,
) -> Result<(), IxaError> {
    // Create itinerary entries for all setting memberships in input file
    let setting_category: String = setting_record.settingcategory.to_string();
    let setting_code: String = String::from_utf8(setting_record.settingcode.to_owned())?;

    // let fips_code: usize = setting_code.parse()?;
    // let geography = GeographyProperties { fips_code };
        // let fips = get_fips_from_string(setting_string.clone())?;
        let alpha = 0.0;
        let itinerary_ratio = 0.25;
    let _setting_id = match setting_category.as_str() {
        "homeId" => context.add_entity::<SettingEntity, _>((
            SettingCode(setting_code.parse()?),
            SettingCategory::Home,
            Alpha(alpha),
            DefaultItineraryRatio(itinerary_ratio),
        ))?,
        "workplaceId" => context.add_entity::<SettingEntity, _>((
            SettingCode(setting_code.parse()?),
            SettingCategory::Workplace,
            Alpha(alpha),
            DefaultItineraryRatio(itinerary_ratio),
        ))?,
        "schoolId" => context.add_entity::<SettingEntity, _>((
            SettingCode(setting_code.parse()?),
            SettingCategory::School,
            Alpha(alpha),
            DefaultItineraryRatio(itinerary_ratio),
        ))?,
        "censustractId" => context.add_entity::<SettingEntity, _>((
            SettingCode(setting_code.parse()?),
            SettingCategory::CensusTract,
            Alpha(alpha),
            DefaultItineraryRatio(itinerary_ratio),
        ))?,
        _ => {
            return Err(IxaError::IxaError(format!(
                "Invalid setting category {} in settings file",
                setting_category
            )));
        }
    };

    Ok(())
}

pub fn load_settings(
    context: &mut Context,
    setting_file: PathBuf,
) -> Result<(), IxaError> {
    let mut reader = csv::Reader::from_path(setting_file)?;
    let mut raw_record = csv::ByteRecord::new();
    let headers = reader.byte_headers()?.clone();

    while reader.read_byte_record(&mut raw_record)? {
        let record: SettingRecord = raw_record.deserialize(Some(&headers))?;
        create_setting_from_record(
            context,
            &record,
        )?;
    }
    Ok(())
}

pub fn init(context: &mut Context) -> Result<(), IxaError> {
    let _span = open_span("load_setting_population");
    let Params {
        settings_file,
        ..
    } = context.get_params();
    load_settings(
        context,
        settings_file.clone(),
    )?;
    context.index_property::<SettingEntity, SettingCode>();
    Ok(())
}