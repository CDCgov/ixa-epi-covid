use std::ops::{Index, IndexMut};

use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self},
};
use strum::{EnumCount, EnumIter};

use crate::pop_reader::{FIPSCode, StateCode, parser::parse_fips_state_county_id};

#[derive(Deserialize)]
#[serde(tag = "type", content = "code")]
enum RawGeography {
    #[serde(rename = "county", alias = "County", alias = "COUNTY")]
    County(String),

    #[serde(rename = "state", alias = "State", alias = "STATE")]
    State(String),
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Eq, Hash, EnumCount, EnumIter)]
#[repr(u8)]
pub enum Geography {
    County(FIPSCode) = 0,
    State(StateCode),
}

impl Geography {
    const fn index(self) -> usize {
        match self {
            Geography::County(_) => 0,
            Geography::State(_) => 1,
        }
    }
}

impl<T> Index<Geography> for [T; GEOGRAPHY_COUNT] {
    type Output = T;

    fn index(&self, index: Geography) -> &Self::Output {
        &self[index.index()]
    }
}

impl<T> IndexMut<Geography> for [T; GEOGRAPHY_COUNT] {
    fn index_mut(&mut self, index: Geography) -> &mut Self::Output {
        &mut self[index.index()]
    }
}

pub const GEOGRAPHY_COUNT: usize = Geography::COUNT;

impl<'de> Deserialize<'de> for Geography {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawGeography::deserialize(deserializer)?;

        match raw {
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_deserialize_geography() {
        let json = r#"{"type": "county", "code": "01001"}"#;
        let geography: Geography = serde_json::from_str(json).unwrap();
        assert_eq!(
            geography,
            Geography::County(parse_fips_state_county_id("01001".as_bytes()).unwrap().1)
        );

        let json = r#"{"type": "state", "code": "01"}"#;
        let geography: Geography = serde_json::from_str(json).unwrap();
        assert_eq!(geography, Geography::State(1));
    }
}
