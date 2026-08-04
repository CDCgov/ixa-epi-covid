use std::cmp::Ordering;

use ixa::prelude::*;
use serde::{Deserialize, Serialize};
use strum::{EnumDiscriminants, EnumIter, FromRepr, IntoStaticStr};

define_entity!(Region);

#[derive(
    Copy,
    Clone,
    PartialEq,
    Debug,
    Deserialize,
    Serialize,
    Eq,
    Hash,
    FromRepr,
    EnumIter,
    EnumDiscriminants,
)]
#[strum_discriminants(name(GeographyType))]
#[strum_discriminants(derive(PartialOrd, Ord, Hash, Deserialize, Serialize))]
#[strum_discriminants(derive(IntoStaticStr), repr(u8))]
pub enum Geography {
    CensusTract(u8, u16, u32),
    County(u8, u16),
    State(u8),
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

    pub fn overlaps(&self, other: &Self) -> bool {
        match (self, other) {
            // Same type
            (Self::State(s1), Self::State(s2)) => s1 == s2,

            (Self::County(s1, c1), Self::County(s2, c2)) => s1 == s2 && c1 == c2,

            (Self::CensusTract(s1, c1, t1), Self::CensusTract(s2, c2, t2)) => {
                s1 == s2 && c1 == c2 && t1 == t2
            }

            // State contains County
            (Self::State(state), Self::County(other_state, _))
            | (Self::County(other_state, _), Self::State(state)) => state == other_state,

            // State contains CensusTract
            (Self::State(state), Self::CensusTract(other_state, _, _))
            | (Self::CensusTract(other_state, _, _), Self::State(state)) => state == other_state,

            // County contains CensusTract
            (Self::County(state, county), Self::CensusTract(other_state, other_county, _))
            | (Self::CensusTract(other_state, other_county, _), Self::County(state, county)) => {
                state == other_state && county == other_county
            }
        }
    }
}

impl_property!(Geography, Region);

pub trait ContextGeographyExt: PluginContext {
    fn create_region(&mut self, geography: Geography) -> RegionId {
        self.add_entity(with!(Region, geography)).unwrap()
    }

    fn region_overlap(&self, region: RegionId, other: RegionId) -> bool {
        let geography = self.get_property::<Region, Geography>(region);
        let other_geography = self.get_property::<Region, Geography>(other);
        geography.overlaps(&other_geography)
    }

    fn filter_overlapping_regions(
        &self,
        region: RegionId,
        regions: &[RegionId],
    ) -> Option<Vec<RegionId>> {
        let geography = self.get_property::<Region, Geography>(region);
        let filtered = regions
            .iter()
            .copied()
            .filter(|&other_region| {
                let other_geography = self.get_property::<Region, Geography>(other_region);
                geography.overlaps(&other_geography)
            })
            .collect::<Vec<RegionId>>();
        (!filtered.is_empty()).then_some(filtered)
    }

    fn filter_largest_nonoverlapping_regions(&self, regions: Vec<RegionId>) -> Vec<RegionId> {
        let mut geographies: Vec<(RegionId, Geography)> = regions
            .into_iter()
            .map(|region| {
                let geography = self.get_property::<Region, Geography>(region);
                (region, geography)
            })
            .collect();

        // Stable sorting preserves input order among equal-sized geographies.
        geographies.sort_by_key(|(_, geography)| std::cmp::Reverse(geography.geography_type_u8()));

        let mut selected = Vec::new();

        for (region, geography) in geographies {
            let overlaps_selected = selected.iter().any(|&selected_region| {
                let selected_geography = self.get_property::<Region, Geography>(selected_region);
                geography.overlaps(&selected_geography)
            });

            if !overlaps_selected {
                selected.push(region);
            }
        }

        selected
    }
}
impl ContextGeographyExt for Context {}

#[cfg(test)]
mod test {
    use crate::{pop_reader::parser::parse_fips_school_id, settings::SettingCode};

    use super::*;
    #[allow(dead_code)]
    fn make_school_id(school_id: &[u8]) -> SettingCode {
        SettingCode(parse_fips_school_id(school_id).unwrap().1)
    }

    #[test]
    fn test_overlap() {
        let mut context = Context::new();
        let r1 = context
            .add_entity(with!(Region, Geography::State(1)))
            .unwrap();
        let r2 = context
            .add_entity(with!(Region, Geography::County(1, 2)))
            .unwrap();
        let r3 = context
            .add_entity(with!(Region, Geography::CensusTract(1, 2, 3)))
            .unwrap();
        let r4 = context
            .add_entity(with!(Region, Geography::State(2)))
            .unwrap();
        assert!(context.region_overlap(r1, r2));
        assert!(context.region_overlap(r1, r3));
        assert!(context.region_overlap(r2, r3));
        assert!(!context.region_overlap(r1, r4));
        assert!(!context.region_overlap(r2, r4));
        assert!(!context.region_overlap(r3, r4));
    }

    #[test]
    fn test_filter_overlap() {
        let mut context = Context::new();
        let r1 = context
            .add_entity(with!(Region, Geography::State(1)))
            .unwrap();
        let r2 = context
            .add_entity(with!(Region, Geography::County(1, 2)))
            .unwrap();
        let r3 = context
            .add_entity(with!(Region, Geography::CensusTract(1, 2, 3)))
            .unwrap();
        let r4 = context
            .add_entity(with!(Region, Geography::State(2)))
            .unwrap();
        let existing_regions = vec![r1, r2, r3, r4];
        assert_eq!(
            context.filter_overlapping_regions(r1, &existing_regions),
            Some(vec![r1, r2, r3])
        );
        assert_eq!(
            context.filter_overlapping_regions(r2, &existing_regions),
            Some(vec![r1, r2, r3])
        );
        assert_eq!(
            context.filter_overlapping_regions(r3, &existing_regions),
            Some(vec![r1, r2, r3])
        );
        assert_eq!(
            context.filter_overlapping_regions(r4, &existing_regions),
            Some(vec![r4])
        );
    }

    #[test]
    fn test_filter_largest_nonoverlapping() {
        let mut context = Context::new();
        let r1 = context
            .add_entity(with!(Region, Geography::State(1)))
            .unwrap();
        let r2 = context
            .add_entity(with!(Region, Geography::County(1, 2)))
            .unwrap();
        let r3 = context
            .add_entity(with!(Region, Geography::CensusTract(3, 2, 3)))
            .unwrap();
        let r4 = context
            .add_entity(with!(Region, Geography::State(2)))
            .unwrap();
        let r5 = context
            .add_entity(with!(Region, Geography::County(2, 1)))
            .unwrap();
        let r6 = context
            .add_entity(with!(Region, Geography::CensusTract(2, 2, 3)))
            .unwrap();
        let regions = vec![r1, r2, r3, r4, r5, r6];
        let regions2 = vec![r5, r6];
        let filtered = context.filter_largest_nonoverlapping_regions(regions);
        assert_eq!(filtered, vec![r1, r4, r3]);
        let filtered2 = context.filter_largest_nonoverlapping_regions(regions2);
        assert_eq!(filtered2, vec![r5, r6]);
    }

    #[test]
    #[allow(clippy::nonminimal_bool)]
    fn test_geography_ordering() {
        let g1 = Geography::State(1);
        let g2 = Geography::County(1, 2);
        let g3 = Geography::CensusTract(1, 2, 3);
        let g4 = Geography::State(2);
        assert!(g1 > g2);
        assert!(g2 > g3);
        assert!(g1 > g3);
        assert!(!(g1 < g4) && !(g4 > g1));
    }
}
