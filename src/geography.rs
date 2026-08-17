use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self, Visitor},
};
use std::{cmp::Ordering, fmt};
use strum::{EnumDiscriminants, IntoStaticStr};

use crate::{
    error::ModelError,
    pop_reader::{FIPSCode, StateCode, parser::parse_fips_state_county_id},
    schools::school_district::LEACode,
};

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Eq, Hash)]
pub struct FIPSStateCountyCode(pub [u8; 5]);

impl<'de> Deserialize<'de> for FIPSStateCountyCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct FIPSStateCountyCodeVisitor;

        impl<'de> Visitor<'de> for FIPSStateCountyCodeVisitor {
            type Value = FIPSStateCountyCode;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a 5-digit number")
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if !(10_000..=99_999).contains(&value) {
                    return Err(E::custom("expected a 5-digit number"));
                }

                let mut digits = [0u8; 5];
                let mut value = value;

                for index in (0..5).rev() {
                    digits[index] = (value % 10) as u8;
                    value /= 10;
                }

                Ok(FIPSStateCountyCode(digits))
            }
        }

        deserializer.deserialize_u64(FIPSStateCountyCodeVisitor)
    }
}

impl FIPSStateCountyCode {
    pub fn as_ascii(&self) -> Vec<u8> {
        self.0.iter().map(|digit| b'0' + digit).collect()
    }
}

#[derive(Copy, Clone, PartialEq, Debug, Deserialize, Serialize, Eq, Hash, EnumDiscriminants)]
#[strum_discriminants(name(GeographyType))]
#[strum_discriminants(derive(PartialOrd, Ord, Hash, Deserialize, Serialize))]
#[strum_discriminants(derive(IntoStaticStr), repr(u8))]
#[serde(tag = "geography_type", content = "code")]
pub enum Geography {
    #[strum_discriminants(serde(
        rename = "schooldistrict",
        alias = "school district",
        alias = "school_district",
        alias = "SCHOOL_DISTRICT"
    ))]
    SchoolDistrict(LEACode),

    #[strum_discriminants(serde(
        rename = "censustract",
        alias = "census tract",
        alias = "census_tract",
        alias = "CENSUS_TRACT"
    ))]
    CensusTract(FIPSCode),

    #[strum_discriminants(serde(rename = "county", alias = "County", alias = "COUNTY"))]
    County(FIPSStateCountyCode),

    #[strum_discriminants(serde(rename = "state", alias = "State", alias = "STATE"))]
    State(StateCode),
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
            | (Geography::County(fips2), Geography::CensusTract(fips1)) => {
                let fips_county = parse_fips_state_county_id(&fips2.as_ascii()).unwrap().1;
                Ok(fips1.state_code() == fips_county.state_code()
                    && fips1.county_code() == fips_county.county_code())
            }
            (Geography::CensusTract(fips1), Geography::State(state2))
            | (Geography::State(state2), Geography::CensusTract(fips1)) => {
                Ok(fips1.state_code() == *state2)
            }

            (Geography::County(fips1), Geography::County(fips2)) => Ok(fips1 == fips2),
            (Geography::County(fips1), Geography::State(state2))
            | (Geography::State(state2), Geography::County(fips1)) => {
                let fips_county = parse_fips_state_county_id(&fips1.as_ascii()).unwrap().1;
                Ok(fips_county.state_code() == *state2)
            }

            (Geography::State(state1), Geography::State(state2)) => Ok(state1 == state2),

            // For SchoolDistrict, we don't have a mapping to FIPS codes, so we can't determine overlaps with other geographies.
            _ => Err(ModelError::ModelError(
                "Cannot determine overlaps for SchoolDistrict geography".to_string(),
            )),
        }
    }
}

#[cfg(test)]
mod test {
    use crate::pop_reader::parser::parse_fips_community_id;

    use super::*;

    #[test]
    #[allow(clippy::nonminimal_bool)]
    fn test_geography_ordering() {
        let community_id = parse_fips_community_id(b"01001020100").unwrap().1;
        let g1 = Geography::State(1);
        let g3 = Geography::CensusTract(community_id);
        let g4 = Geography::State(2);
        assert!(g1 > g3);
        assert!(!(g1 < g4) && !(g4 > g1));
    }
}
