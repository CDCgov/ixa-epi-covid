use std::{fmt, fs::File, path::PathBuf};

use crate::{
    error::ModelError,
    geography::Geography,
    pop_reader::parser::parse_fips_community_id,
    schools::school_closure::{SchoolClosureParameters, SchoolClosureRecords},
};
use ixa::{HashMap, HashMapExt};
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self, Visitor},
};

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Eq, Hash)]
pub struct LEACode(pub [u8; 7]);

impl<'de> Deserialize<'de> for LEACode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct LEACodeVisitor;

        impl<'de> Visitor<'de> for LEACodeVisitor {
            type Value = LEACode;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a 7-digit number")
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if !(1_000_000..=9_999_999).contains(&value) {
                    return Err(E::custom("expected a 7-digit number"));
                }

                let mut digits = [0u8; 7];
                let mut value = value;

                for index in (0..7).rev() {
                    digits[index] = (value % 10) as u8;
                    value /= 10;
                }

                Ok(LEACode(digits))
            }
        }

        deserializer.deserialize_u64(LEACodeVisitor)
    }
}

impl LEACode {
    pub fn as_string(&self) -> String {
        self.0
            .iter()
            .map(|digit| char::from(b'0' + *digit))
            .collect()
    }
}

pub fn process_school_closure_records(
    school_closures: SchoolClosureRecords,
) -> Result<Vec<SchoolClosureParameters>, ModelError> {
    let mut processed_records: Vec<SchoolClosureParameters> = Vec::new();
    let mapping = read_mapping(school_closures.district_mapping)?;
    for record in school_closures.records {
        match record.geography {
            Geography::SchoolDistrict(code) => {
                let fips_codes = mapping.get(&code).ok_or_else(|| {
                    ModelError::ModelError(format!("No FIPS codes found for LEA code: {:?}", code))
                })?;
                for fips_code in fips_codes {
                    let converted_fips_code =
                        parse_fips_community_id(fips_code.as_bytes()).unwrap().1;
                    processed_records.push(SchoolClosureParameters {
                        geography: Geography::CensusTract(converted_fips_code),
                        start_time: record.start_time,
                        end_time: record.end_time,
                    });
                }
            }
            Geography::CensusTract(fips_code) => {
                processed_records.push(SchoolClosureParameters {
                    geography: Geography::CensusTract(fips_code),
                    start_time: record.start_time,
                    end_time: record.end_time,
                });
            }
            Geography::County(fips_code) => {
                processed_records.push(SchoolClosureParameters {
                    geography: Geography::County(fips_code),
                    start_time: record.start_time,
                    end_time: record.end_time,
                });
            }
            Geography::State(state_code) => {
                processed_records.push(SchoolClosureParameters {
                    geography: Geography::State(state_code),
                    start_time: record.start_time,
                    end_time: record.end_time,
                });
            }
        }
    }
    Ok(processed_records)
}

fn read_mapping(
    mapping_path: Option<PathBuf>,
) -> Result<HashMap<LEACode, Vec<String>>, ModelError> {
    if let Some(path) = mapping_path {
        let file = File::open(path)?;
        let mapping: HashMap<LEACode, Vec<String>> = serde_json::from_reader(file)?;
        Ok(mapping)
    } else {
        Ok(HashMap::new())
    }
}
