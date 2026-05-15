use ixa::{HashMap, csv};
use ixa::{prelude::*, profiling::open_span};

use serde::{Deserialize};
use std::path::PathBuf;

use crate::Params;
use crate::error::ModelError;
use crate::parameters::ContextParametersExt;

use crate::pop_reader::{CountyCode, FIPSCode, StateCode, TractCode};


define_rng!(MobilityRng);

/// A parsed flow record from the supported CSV format.
#[derive(Copy, Clone, PartialEq, Debug, Deserialize)]
pub struct FlowRecord {
    pub from_state: StateCode,
    pub from_county: CountyCode,
    pub from_tract: TractCode,
    pub to_state: StateCode,
    pub to_county: CountyCode,
    pub to_tract: TractCode,
    pub flow: f64,
}

fn add_mobility_flow_data(
    context: &mut Context,
    flow_record: FlowRecord,
) -> Result<(), ModelError> {
    
    let from_state = flow_record.from_state;
    let from_county = flow_record.from_county;
    let from_tract = flow_record.from_tract;
    let to_state = flow_record.to_state;
    let to_county = flow_record.to_county;
    let to_tract = flow_record.to_tract;
    let from_census_tract = FIPSCode::with_category(
        from_state,
        from_county,
        from_tract,
        crate::pop_reader::PopulationReaderSettingCategory::CensusTract.encode(),
    )?;
    let to_census_tract = FIPSCode::with_category(
        to_state,
        to_county,
        to_tract,
        crate::pop_reader::PopulationReaderSettingCategory::CensusTract.encode(),
    )?;
    let probability = flow_record.flow;
    context.add_flow(from_census_tract, to_census_tract, probability);
    Ok(())

}

fn load_mobility_flow(
    context: &mut Context,
    mobility_flow_file: PathBuf,
) -> Result<(), ModelError> {
    let mut reader = csv::Reader::from_path(mobility_flow_file)?;
    let mut raw_record = csv::ByteRecord::new();
    let headers = reader.byte_headers()?.clone();

    while reader.read_byte_record(&mut raw_record)? {
        let record: FlowRecord = raw_record.deserialize(Some(&headers))?;
        add_mobility_flow_data(context, record)?;
    }
    Ok(())
}

/// An index of settings as represented by their setting codes.
#[derive(Default)]
pub struct FlowData {
    probabilities: HashMap<FIPSCode, Vec<f64>>,
    destinations: HashMap<FIPSCode, Vec<FIPSCode>>,
}

impl FlowData {
    pub fn new() -> Self {
        Self {
            probabilities: HashMap::default(),
            destinations: HashMap::default(),
        }
    }

    pub fn add_flow(&mut self, from: FIPSCode, to: FIPSCode, probability: f64) {
        let flows = self.probabilities.entry(from).or_default();
        flows.push(probability);
        let dests = self.destinations.entry(from).or_default();
        dests.push(to);
    }

    pub fn get_flows(&self, from: FIPSCode) -> Option<(&Vec<FIPSCode>, &Vec<f64>)> {
        Some((self.destinations.get(&from)?, self.probabilities.get(&from)?))
    }

}

define_data_plugin!(
    FlowDataPlugin,
    FlowData,
    FlowData::default()
);

pub trait ContextMobilityExt: PluginContext + ContextRandomExt{
    fn add_flow(&mut self, from: FIPSCode, to: FIPSCode, probability: f64) {
        let flow_data = self.get_data_mut(FlowDataPlugin);
        flow_data.add_flow(from, to, probability);
    }
    
    fn sample_desitination(&self, from: FIPSCode) -> Option<FIPSCode> {
        let flow_data = self.get_data(FlowDataPlugin);
        let (destinations, weights) = flow_data.get_flows(from)?;
        let setting_index = self.sample_weighted(MobilityRng, weights.as_slice());
        return Some(destinations[setting_index]);
    }
}
impl ContextMobilityExt for Context {}

pub fn init(
    context: &mut Context,
) -> Result<(), ModelError> {
    let _span = open_span("load_mobility_flow");
    let Params {
        mobility_flow_file,
        ..
    } = context.get_params();
    load_mobility_flow(context, mobility_flow_file.clone())?;
    Ok(())
}

#[allow(dead_code)]
#[cfg(test)]
mod test {
    use ixa::HashMap;

use super::*;
    use crate::{parameters::{GlobalParams, Params, SettingProperties}, settings::SettingCategory};
    
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

}
