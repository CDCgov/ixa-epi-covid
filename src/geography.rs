use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use strum::{EnumDiscriminants, IntoStaticStr};

use crate::{pop_reader::{FIPSCode, StateCode}, school_closure::school_district::LEACode};

#[derive(Copy, Clone, PartialEq, Debug, Deserialize, Serialize, Eq, Hash, EnumDiscriminants)]
#[strum_discriminants(name(GeographyType))]
#[strum_discriminants(derive(PartialOrd, Ord, Hash, Deserialize, Serialize))]
#[strum_discriminants(derive(IntoStaticStr), repr(u8))]
#[serde(tag = "geography_type", content = "code")]
pub enum Geography {
    #[strum_discriminants(serde(rename = "schooldistrict", alias = "school district", alias = "school_district", alias = "SCHOOL_DISTRICT"))]
    SchoolDistrict(LEACode),
    
    #[strum_discriminants(serde(
        rename = "censustract",
        alias = "census tract",
        alias = "census_tract",
        alias = "CENSUS_TRACT"
    ))]
    CensusTract(FIPSCode),

    #[strum_discriminants(serde(rename = "county", alias = "County", alias = "COUNTY"))]
    County(FIPSCode),

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
