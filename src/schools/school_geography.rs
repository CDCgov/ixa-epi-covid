use std::{fs::File, path::PathBuf};

use crate::{
    error::ModelError,
    pop_reader::parser::parse_fips_state_county_tract_id,
    schools::school_closure::{SchoolClosureModifier, SchoolClosureParameters},
};
use ixa::{HashMap, HashMapExt};
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self},
};

use std::cmp::Ordering;
use strum::{EnumDiscriminants, IntoStaticStr};

use crate::pop_reader::{FIPSCode, StateCode, parser::parse_fips_state_county_id};

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Eq, Hash, Deserialize)]
pub struct LEACode(pub [u8; 7]);

impl LEACode {
    pub fn as_string(&self) -> String {
        self.0
            .iter()
            .map(|digit| char::from(b'0' + *digit))
            .collect()
    }
}

#[derive(Deserialize)]
#[serde(tag = "type", content = "code")]
enum RawGeography {
    #[serde(
        rename = "schooldistrict",
        alias = "SchoolDistrict",
        alias = "school_district",
        alias = "SCHOOL_DISTRICT"
    )]
    SchoolDistrict(String),

    #[serde(
        rename = "censustract",
        alias = "census tract",
        alias = "census_tract",
        alias = "CENSUS_TRACT"
    )]
    CensusTract(String),

    #[serde(rename = "county", alias = "County", alias = "COUNTY")]
    County(String),

    #[serde(rename = "state", alias = "State", alias = "STATE")]
    State(String),
}

pub fn as_ascii(digits: &[u8]) -> Vec<u8> {
    digits.iter().map(|digit| b'0' + digit).collect()
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Eq, Hash, EnumDiscriminants)]
#[strum_discriminants(name(GeographyType))]
#[strum_discriminants(derive(PartialOrd, Ord, Hash, Deserialize, Serialize))]
#[strum_discriminants(derive(IntoStaticStr), repr(u8))]
pub enum Geography {
    SchoolDistrict(LEACode),
    CensusTract(FIPSCode),
    County(FIPSCode),
    State(StateCode),
}

impl<'de> Deserialize<'de> for Geography {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawGeography::deserialize(deserializer)?;

        match raw {
            RawGeography::SchoolDistrict(code) => Ok(Geography::SchoolDistrict(LEACode(
                code.chars()
                    .map(|c| c.to_digit(10).unwrap() as u8)
                    .collect::<Vec<u8>>()
                    .try_into()
                    .map_err(|_| de::Error::custom("LEA code must be exactly 7 digits"))?,
            ))),

            RawGeography::CensusTract(code) => Ok(Geography::CensusTract(
                parse_fips_state_county_tract_id(code.as_bytes()).unwrap().1,
            )),

            RawGeography::State(code) => {
                let state_code = code
                    .parse::<u8>()
                    .map_err(|_| de::Error::custom("state code must be a valid u8"))?;

                Ok(Geography::State(state_code))
            }

            RawGeography::County(code) => {
                let (rest, fips_code) =
                    parse_fips_state_county_id(code.as_bytes()).map_err(|error| {
                        de::Error::custom(format!("invalid FIPS county code: {error:?}"))
                    })?;

                if !rest.is_empty() {
                    return Err(de::Error::custom("unexpected data after FIPS county code"));
                }

                Ok(Geography::County(fips_code))
            }
        }
    }
}

impl PartialOrd for Geography {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Geography {
    fn cmp(&self, other: &Self) -> Ordering {
        self.geography_type_u8().cmp(&other.geography_type_u8())
    }
}

impl Geography {
    fn geography_type(&self) -> GeographyType {
        GeographyType::from(*self)
    }

    fn geography_type_u8(&self) -> u8 {
        self.geography_type() as u8
    }

    pub fn overlaps(&self, other: &Self) -> Result<bool, ModelError> {
        match (self, other) {
            (Geography::CensusTract(fips1), Geography::CensusTract(fips2)) => Ok(fips1 == fips2),
            (Geography::CensusTract(fips1), Geography::County(fips2))
            | (Geography::County(fips2), Geography::CensusTract(fips1)) => Ok(fips1.state_code()
                == fips2.state_code()
                && fips1.county_code() == fips2.county_code()),
            (Geography::CensusTract(fips1), Geography::State(state2))
            | (Geography::State(state2), Geography::CensusTract(fips1)) => {
                Ok(fips1.state_code() == *state2)
            }

            (Geography::County(fips1), Geography::County(fips2)) => Ok(fips1 == fips2),
            (Geography::County(fips1), Geography::State(state2))
            | (Geography::State(state2), Geography::County(fips1)) => {
                Ok(fips1.state_code() == *state2)
            }

            (Geography::State(state1), Geography::State(state2)) => Ok(state1 == state2),

            // For SchoolDistrict, we don't have a mapping to FIPS codes, so we can't determine overlaps with other geographies.
            _ => Err(ModelError::ModelError(
                "Cannot determine overlaps for SchoolDistrict geography".to_string(),
            )),
        }
    }
}

fn expand_school_district(
    code: LEACode,
    mapping: &HashMap<String, Vec<String>>,
) -> Result<Vec<FIPSCode>, ModelError> {
    let fips_codes = mapping.get(&code.as_string()).ok_or_else(|| {
        ModelError::ModelError(format!("No FIPS codes found for LEA code: {:?}", code))
    })?;

    fips_codes
        .iter()
        .map(|fips_code| {
            parse_fips_state_county_tract_id(fips_code.as_bytes())
                .map(|(_, code)| code)
                .map_err(|error| {
                    ModelError::ModelError(format!(
                        "Invalid FIPS tract code {fips_code:?}: {error:?}"
                    ))
                })
        })
        .collect()
}

pub fn process_school_closure_records(
    school_closures: SchoolClosureModifier,
) -> Result<Vec<SchoolClosureParameters>, ModelError> {
    let mut processed_records: Vec<SchoolClosureParameters> = Vec::new();
    let mapping: HashMap<String, Vec<String>> = read_mapping(school_closures.district_mapping)?;
    for record in school_closures.closures {
        match record.geography {
            Geography::SchoolDistrict(code) => {
                if mapping.is_empty() {
                    return Err(ModelError::ModelError(
                        "No school district mapping entries were provided".to_string(),
                    ));
                }
                let fips_codes = expand_school_district(code, &mapping)?;
                for fips_code in fips_codes {
                    processed_records.push(SchoolClosureParameters {
                        geography: Geography::CensusTract(fips_code),
                        activates_at: record.activates_at,
                        deactivates_at: record.deactivates_at,
                    });
                }
            }
            Geography::CensusTract(_) => {
                return Err(ModelError::ModelError(
                    "Census tract closures are not supported.".to_string(),
                ));
            }
            _ => {
                processed_records.push(record);
            }
        }
    }
    Ok(processed_records)
}

fn read_mapping(mapping_path: Option<PathBuf>) -> Result<HashMap<String, Vec<String>>, ModelError> {
    if let Some(path) = mapping_path {
        let file = File::open(path)?;
        let mapping: HashMap<String, Vec<String>> = serde_json::from_reader(file)?;
        Ok(mapping)
    } else {
        Ok(HashMap::new())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::schools::school_closure::SchoolClosureModifier;
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
                    geography: Geography::County(parse_fips_state_county_id(b"12345").unwrap().1),
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
            Geography::County(parse_fips_state_county_id(b"12345").unwrap().1)
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
            Geography::CensusTract(parse_fips_state_county_tract_id(b"01001020100").unwrap().1)
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
                    parse_fips_state_county_tract_id(b"01001020100").unwrap().1,
                ),
                activates_at: 10.0,
                deactivates_at: 20.0,
            }],
        };

        let error = process_school_closure_records(school_closures).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Census tract closures are not supported.")
        );
    }

    #[test]
    #[allow(clippy::nonminimal_bool)]
    fn test_geography_ordering() {
        let community_id = parse_fips_state_county_tract_id(b"01001020100").unwrap().1;
        let g1 = Geography::State(1);
        let g3 = Geography::CensusTract(community_id);
        let g4 = Geography::State(2);
        assert!(g1 > g3);
        assert!(!(g1 < g4) && !(g4 > g1));
    }

    #[test]
    fn test_geography_overlaps() {
        let g1 = Geography::State(1);
        let g2 = Geography::County(parse_fips_state_county_id(b"01001").unwrap().1);
        let g3 =
            Geography::CensusTract(parse_fips_state_county_tract_id(b"01001020100").unwrap().1);
        let g4 = Geography::County(parse_fips_state_county_id(b"02001").unwrap().1);
        let g5 =
            Geography::CensusTract(parse_fips_state_county_tract_id(b"02001020200").unwrap().1);
        assert!(g1.overlaps(&g2).unwrap());
        assert!(g1.overlaps(&g3).unwrap());
        assert!(!g1.overlaps(&g4).unwrap());
        assert!(!g1.overlaps(&g5).unwrap());
        assert!(g2.overlaps(&g3).unwrap());
        assert!(!g2.overlaps(&g4).unwrap());
        assert!(!g2.overlaps(&g5).unwrap());
        assert!(!g3.overlaps(&g4).unwrap());
        assert!(!g3.overlaps(&g5).unwrap());
        assert!(g4.overlaps(&g5).unwrap());
    }
}
