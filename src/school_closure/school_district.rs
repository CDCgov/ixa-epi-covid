use std::{fs::File, path::PathBuf};

use crate::{error::ModelError, geography::Geography, pop_reader::FIPSCode, school_closure::school_closure::{SchoolClosureParameters, SchoolClosureRecords}};
use ixa::{HashMap, prelude::*, HashMapExt};

pub type LEACode = [u8; 7];
pub fn process_school_closure_records(
    school_closures: SchoolClosureRecords,
 ) -> Result<Vec<SchoolClosureParameters>, ModelError> {
     let mut processed_records: Vec<SchoolClosureParameters> = Vec::new();
     let mapping = read_mapping(school_closures.mapping_path)?;
     for record in school_closures.records {
        match record.geography {
            Geography::SchoolDistrict(code) => {
                if let Some(lea_code) = record.lea_code {
                    processed_records.push(SchoolClosureParameters {
                        geography: Geography::SchoolDistrict(lea_code),
                        start_condition: record.start_condition,
                        end_conditions: record.end_conditions,
                    });
                } else {
                    return Err(ModelError::InvalidSchoolClosureRecord(
                        "Missing LEA code for school district closure".to_string(),
                    ));
                }
            }
            Geography::CensusTract(fips_code) => {
                processed_records.push(SchoolClosureParameters {
                    geography: Geography::CensusTract(fips_code),
                    start_condition: record.start_condition,
                    end_conditions: record.end_conditions,
                });
            }
            Geography::County(fips_code) => {
                processed_records.push(SchoolClosureParameters {
                    geography: Geography::County(fips_code),
                    start_condition: record.start_condition,
                    end_conditions: record.end_conditions,
                });
            }
            Geography::State(state_code) => {
                processed_records.push(SchoolClosureParameters {
                    geography: Geography::State(state_code),
                    start_condition: record.start_condition,
                    end_conditions: record.end_conditions,
                });
            }
        }
     }
     Ok(processed_records)
 }

fn read_mapping(mapping_path: Option<PathBuf>) -> Result<HashMap<LEACode, Vec<FIPSCode>>, ModelError> {
    if let Some(path) = mapping_path {
        let file = File::open(path)?;
        let mapping: HashMap<LEACode, Vec<FIPSCode>> =
            serde_json::from_reader(file)?;
        Ok(mapping)
    } else {
        Ok(HashMap::new())
    }
}