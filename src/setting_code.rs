//! Setting codes are used to identify settings in the population. The `SettingCode` type is
//! a newtype for `FIPSCode`. Thus, it knows its setting category, state, county, and tract.

use crate::pop_reader::{FIPSCode, PopulationReaderSettingCategory};
use crate::settings::SettingCategory;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone, Copy, Hash)]
pub struct SettingCode(pub FIPSCode);

impl SettingCode {
    pub fn category(&self) -> SettingCategory {
        match PopulationReaderSettingCategory::from_repr(self.0.category_code()) {
            Some(PopulationReaderSettingCategory::Home) => SettingCategory::Home,
            Some(PopulationReaderSettingCategory::PrivateSchool)
            | Some(PopulationReaderSettingCategory::PublicSchool) => SettingCategory::School,
            Some(PopulationReaderSettingCategory::Workplace) => SettingCategory::Work,
            Some(PopulationReaderSettingCategory::CensusTract) => SettingCategory::Community,
            _ => panic!("Invalid setting category code: {}", self.0.category_code()),
        }
    }

    pub fn extract_community(&self) -> Self {
        let home_id = self.0;
        // Since we are calling this constructor with values that we know are valid, we can unwrap.
        SettingCode(
            FIPSCode::with_category(
                home_id.state_code(),
                home_id.county_code(),
                home_id.census_tract_code(),
                PopulationReaderSettingCategory::CensusTract.encode(),
            )
            .unwrap(),
        )
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::pop_reader::PopulationReaderSettingCategory;
    use crate::pop_reader::fips_code::ExpandedFIPSCode;
    use crate::pop_reader::states::USState;
    use std::sync::atomic::AtomicU16;

    // Helper methods to generate arbitrary SettingCodes for unit tests
    impl SettingCode {
        fn next_home_code() -> u16 {
            static COUNTER: AtomicU16 = AtomicU16::new(0);

            COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        }

        fn next_school_code() -> u16 {
            static COUNTER: AtomicU16 = AtomicU16::new(0);

            COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        }

        fn next_workplace_code() -> u16 {
            static COUNTER: AtomicU16 = AtomicU16::new(0);

            COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        }

        pub fn arbitrary_home_code() -> SettingCode {
            let fips_code = ExpandedFIPSCode {
                state: USState::WY.encode(),
                county: 1,
                tract: 1,
                category: PopulationReaderSettingCategory::Home.encode(),
                id: Self::next_home_code(),
                data: 0,
            }
            .to_fips_code()
            .unwrap();
            SettingCode(fips_code)
        }

        pub fn arbitrary_school_code() -> SettingCode {
            let fips_code = ExpandedFIPSCode {
                state: USState::WY.encode(),
                county: 1,
                tract: 1,
                category: PopulationReaderSettingCategory::PublicSchool.encode(),
                id: Self::next_school_code(),
                data: 0,
            }
            .to_fips_code()
            .unwrap();
            SettingCode(fips_code)
        }

        pub fn arbitrary_workplace_code() -> SettingCode {
            let fips_code = ExpandedFIPSCode {
                state: USState::WY.encode(),
                county: 1,
                tract: 1,
                category: PopulationReaderSettingCategory::Workplace.encode(),
                id: Self::next_workplace_code(),
                data: 0,
            }
            .to_fips_code()
            .unwrap();
            SettingCode(fips_code)
        }

        /// Constructs an arbitrary workplace setting code with the same state, county, and tract
        /// as `self`.
        pub fn as_arbitrary_workplace_code(&self) -> SettingCode {
            let fips_code = self.0;

            let new_fips_code = ExpandedFIPSCode {
                state: fips_code.state_code(),
                county: fips_code.county_code(),
                tract: fips_code.census_tract_code(),
                category: PopulationReaderSettingCategory::Workplace.encode(),
                id: Self::next_workplace_code(),
                data: 0,
            }
            .to_fips_code()
            .unwrap();
            SettingCode(new_fips_code)
        }
    }
}
