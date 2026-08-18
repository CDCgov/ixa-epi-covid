use std::{fmt, fs::File, path::PathBuf};

use crate::{
    error::ModelError,
    geography::Geography,
    pop_reader::parser::parse_fips_community_id,
    schools::school_closure::{SchoolClosureModifier, SchoolClosureParameters},
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

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                let value = value
                    .parse::<u64>()
                    .map_err(|_| E::custom("expected a 7-digit number"))?;
                self.visit_u64(value)
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
    school_closures: SchoolClosureModifier,
) -> Result<Vec<SchoolClosureParameters>, ModelError> {
    let mut processed_records: Vec<SchoolClosureParameters> = Vec::new();
    let mapping = read_mapping(school_closures.district_mapping)?;
    for record in school_closures.closures {
        match record.geography {
            Geography::SchoolDistrict(code) => {
                if mapping.is_empty() {
                    return Err(ModelError::ModelError(
                        "No school district mapping entries were provided".to_string(),
                    ));
                }
                let fips_codes = mapping.get(&code).ok_or_else(|| {
                    ModelError::ModelError(format!("No FIPS codes found for LEA code: {:?}", code))
                })?;
                for fips_code in fips_codes {
                    let converted_fips_code =
                        parse_fips_community_id(fips_code.as_bytes()).unwrap().1;
                    processed_records.push(SchoolClosureParameters {
                        geography: Geography::CensusTract(converted_fips_code),
                        activates_at: record.activates_at,
                        deactivates_at: record.deactivates_at,
                    });
                }
            }
            Geography::CensusTract(fips_code) => {
                Err(ModelError::ModelError(format!(
                    "Census tract closures are not supported: {:?}",
                    fips_code
                )))?;
            }
            Geography::County(fips_code) => {
                processed_records.push(SchoolClosureParameters {
                    geography: Geography::County(fips_code),
                    activates_at: record.activates_at,
                    deactivates_at: record.deactivates_at,
                });
            }
            Geography::State(state_code) => {
                processed_records.push(SchoolClosureParameters {
                    geography: Geography::State(state_code),
                    activates_at: record.activates_at,
                    deactivates_at: record.deactivates_at,
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        geography::{FIPSStateCountyCode, Geography},
        schools::school_closure::SchoolClosureModifier,
    };
    use std::fs;

    #[test]
    fn test_process_school_closure_records() {
        let school_closures = SchoolClosureModifier {
            district_mapping: None,
            closures: vec![
                SchoolClosureParameters {
                    geography: Geography::State(56),
                    activates_at: 0.0,
                    deactivates_at: 10.0,
                },
                SchoolClosureParameters {
                    geography: Geography::County(FIPSStateCountyCode([1, 2, 3, 4, 5])),
                    activates_at: 5.0,
                    deactivates_at: 15.0,
                },
            ],
        };

        let processed_records = process_school_closure_records(school_closures).unwrap();
        assert_eq!(processed_records.len(), 2);
        assert_eq!(processed_records[0].geography, Geography::State(56));
        assert_eq!(
            processed_records[1].geography,
            Geography::County(FIPSStateCountyCode([1, 2, 3, 4, 5]))
        );
        assert!(
            processed_records[0].activates_at == 0.0 && processed_records[0].deactivates_at == 10.0
        );
        assert!(
            processed_records[1].activates_at == 5.0 && processed_records[1].deactivates_at == 15.0
        );
    }

    #[test]
    fn test_read_mapping_json_and_process_districts() {
        let mapping_path = std::env::temp_dir().join(format!(
            "school_district_mapping_{}.json",
            std::process::id()
        ));
        fs::write(&mapping_path, r#"{"1234567":["01001020100"]}"#).unwrap();

        let school_closures = SchoolClosureModifier {
            district_mapping: Some(mapping_path.clone()),
            closures: vec![SchoolClosureParameters {
                geography: Geography::SchoolDistrict(LEACode([1, 2, 3, 4, 5, 6, 7])),
                activates_at: 10.0,
                deactivates_at: 20.0,
            }],
        };

        let processed_records = process_school_closure_records(school_closures).unwrap();
        assert_eq!(processed_records.len(), 1);
        assert_eq!(
            processed_records[0].geography,
            Geography::CensusTract(parse_fips_community_id(b"01001020100").unwrap().1)
        );
        assert!(
            processed_records[0].activates_at == 10.0
                && processed_records[0].deactivates_at == 20.0
        );

        fs::remove_file(mapping_path).unwrap();
    }

    #[test]
    fn test_error_if_no_mapping_for_district() {
        let school_closures = SchoolClosureModifier {
            district_mapping: None,
            closures: vec![SchoolClosureParameters {
                geography: Geography::SchoolDistrict(LEACode([1, 2, 3, 4, 5, 6, 7])),
                activates_at: 10.0,
                deactivates_at: 20.0,
            }],
        };

        let error = process_school_closure_records(school_closures).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("No school district mapping entries were provided")
        );
    }

    #[test]
    fn test_error_for_tract_record() {
        let school_closures = SchoolClosureModifier {
            district_mapping: None,
            closures: vec![SchoolClosureParameters {
                geography: Geography::CensusTract(
                    parse_fips_community_id(b"01001020100").unwrap().1,
                ),
                activates_at: 10.0,
                deactivates_at: 20.0,
            }],
        };

        let error = process_school_closure_records(school_closures).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Census tract closures are not supported:")
        );
    }
}
