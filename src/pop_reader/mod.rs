/*!

This module provides routines for working with a compatible population person-record format.
It includes parsing functionality for the setting identifiers found in that format. The `archive`
submodule additionally reads CSV files in this format, including files within zipped archives.
That submodule addresses source files using a configured data path plus file paths interpreted
relative to that data path; see [`archive`] for the detailed path semantics and examples.

This format encodes `homeId`, `schoolId`, and `workplaceId` using a FIPS geographic region code
prefix. Compatible rows have a single entry for each person with:

1. **Age** as an integer by single year
2. **Home ID** as an 11-digit tract plus a 4-digit within-tract sequential id
3. **School ID** as either:
    - Public: 11-digit tract + 3-digit within-tract sequential id
    - Private: 5-digit county + “xprvx” + 4-digit within-county sequential id
4. **Work ID** as an 11-digit tract plus a 5-digit within-tract sequential id

However, observed data in commonly used datasets is slightly wider in some categories:

- home IDs may use a 5-digit suffix when the within-tract sequence exceeds 9,999
- public-school IDs may use a 4-digit suffix when the within-tract sequence exceeds 999
- private-school IDs are still observed with a 4-digit suffix
- workplace IDs are still observed with a 5-digit suffix

This module therefore treats the geographic prefix as fixed-width, but parses the setting-id suffix as
an integer and requires only that it be nonempty and fit in the `FIPSCode` id field.

These IDs are encoded as a `FIPSCode` struct from `ixa-fips`, an efficient 64-bit representation.
This type is re-exported for convenience.

*/
#![allow(dead_code)]

use std::fmt::{Display, Write};

use strum::FromRepr;

pub use fips_code::FIPSCode;

pub mod archive;
pub mod errors;
pub mod fips_code;
pub mod parser;
pub mod states;

// Numeric types used for code fragments. By convention, zero values are reserved for "no data."

/// The numeric type used for the state code fragment; `u8`
pub type StateCode = u8;
/// The numeric type used for the county code fragment; `u16`
pub type CountyCode = u16;
/// The numeric type used for the tract code fragment; `u32`
pub type TractCode = u32;
/// The numeric type used for the setting category code fragment; `u8`
pub type SettingCategoryCode = u8;
/// The numeric type used for the id code fragment; `u32`
pub type IdCode = u32;
/// The numeric type used for the data code fragment; `u16`
pub type DataCode = u16;

/// A parsed person record from the supported CSV format.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Default, Debug)]
pub struct PersonRecord {
    pub age: u8,
    pub home_id: Option<FIPSCode>,
    pub school_id: Option<FIPSCode>,
    pub work_id: Option<FIPSCode>,
}

impl Display for PersonRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Age: {}", self.age)?;

        if let Some(home) = &self.home_id {
            write!(f, ", Home: ({})", home)?;
        }
        if let Some(school) = &self.school_id {
            write!(f, ", School: ({})", school)?;
        }
        if let Some(work) = &self.work_id {
            write!(f, ", Work: ({})", work)?;
        }

        Ok(())
    }
}

/// A `PopulationReaderSettingCategory` is not itself a FIPS code, but it is implicit in this population record format.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Default, Debug, FromRepr)]
#[repr(u8)]
pub enum PopulationReaderSettingCategory {
    // We expect applications that do not use `SettingCategory` to have this field zeroed out.
    #[default]
    Unspecified = 0,
    Home,
    Workplace,
    PublicSchool,
    PrivateSchool,
    CensusTract,
}

impl PopulationReaderSettingCategory {
    /// Encode a `SettingCategory` as a `u8`
    #[inline(always)]
    pub fn encode(self) -> SettingCategoryCode {
        self as u8
    }
}

impl Display for PopulationReaderSettingCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PopulationReaderSettingCategory::Unspecified => write!(f, "Unspecified"),
            PopulationReaderSettingCategory::Home => write!(f, "Home"),
            PopulationReaderSettingCategory::Workplace => write!(f, "Workplace"),
            PopulationReaderSettingCategory::PublicSchool => write!(f, "Public School"),
            PopulationReaderSettingCategory::PrivateSchool => write!(f, "Private School"),
            PopulationReaderSettingCategory::CensusTract => write!(f, "Census Tract"),
        }
    }
}

/// Serializes a `FIPSCode` value to the same string format expected by the population file
/// reader and parser code. The category code and "data" field are not represented in this format.
/// This should round-trip for supported IDs, including ASPR-compatible dataset values.
fn format_as_fips_code<W: Write>(f: &mut W, fips_code: FIPSCode) -> std::fmt::Result {
    write!(f, "{:02}", fips_code.state_code())?;
    write!(f, "{:03}", fips_code.county_code())?;

    match PopulationReaderSettingCategory::from_repr(fips_code.category_code()) {
        Some(PopulationReaderSettingCategory::Home) => {
            // Published form: 11-digit tract + 4-digit within-tract sequential id.
            // The width is a minimum, so observed 5-digit suffixes are preserved.
            write!(f, "{:06}", fips_code.census_tract_code())?;
            write!(f, "{:04}", fips_code.id())
        }

        Some(PopulationReaderSettingCategory::Workplace) => {
            // Published form: 11-digit tract + 5-digit within-tract sequential id.
            write!(f, "{:06}", fips_code.census_tract_code())?;
            write!(f, "{:05}", fips_code.id())
        }

        Some(PopulationReaderSettingCategory::PublicSchool) => {
            // Published form: 11-digit tract + 3-digit within-tract sequential id.
            // The width is a minimum, so observed 4-digit suffixes are preserved.
            write!(f, "{:06}", fips_code.census_tract_code())?;
            write!(f, "{:03}", fips_code.id())
        }

        Some(PopulationReaderSettingCategory::PrivateSchool) => {
            // Published form: 5-digit county + “xprvx” + 4-digit within-county sequential id
            write!(f, "xprvx")?;
            write!(f, "{:04}", fips_code.id())
        }

        // ToDo: Give a reasonable representation for these categories.
        Some(PopulationReaderSettingCategory::Unspecified)
        | Some(PopulationReaderSettingCategory::CensusTract)
        | None => Err(std::fmt::Error),
    }
    // The category code and "data" field are not represented in this format.
    // write!(f, "{:01}", fips_code.category_code())?;
    // write!(f, "{:03}", fips_code.data())?;
}

/// Serializes a `FIPSCode` value to a `String` in the same format expected by the population
/// file reader and parser code.
fn format_as_fips_code_string(fips_code: FIPSCode) -> String {
    let mut buf = String::new();
    format_as_fips_code(&mut buf, fips_code).unwrap();
    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pop_reader::parser::{
        parse_fips_home_id, parse_fips_school_id, parse_fips_workplace_id,
    };

    #[test]
    fn text_round_trip_formatting() {
        let home_id = "110010109000024";
        let workplace_id = "1100100620201546";
        let public_school_id = "11001009810157";
        let private_school_id = "24031xprvx0085";

        let (_, parsed_home_id) = parse_fips_home_id(home_id.as_bytes()).unwrap();
        let (_, parsed_workplace_id) = parse_fips_workplace_id(workplace_id.as_bytes()).unwrap();
        let (_, parsed_public_school_id) =
            parse_fips_school_id(public_school_id.as_bytes()).unwrap();
        let (_, parsed_private_school_id) =
            parse_fips_school_id(private_school_id.as_bytes()).unwrap();

        assert_eq!(home_id, format_as_fips_code_string(parsed_home_id));
        assert_eq!(
            workplace_id,
            format_as_fips_code_string(parsed_workplace_id)
        );
        assert_eq!(
            public_school_id,
            format_as_fips_code_string(parsed_public_school_id)
        );
        assert_eq!(
            private_school_id,
            format_as_fips_code_string(parsed_private_school_id)
        );
    }

    #[test]
    fn text_round_trip_formatting_for_observed_wider_suffixes() {
        let home_id = "1600101040120507";
        let workplace_id = "1600101040114938";
        let public_school_id = "160010104012789";
        let private_school_id = "24031xprvx1722";

        let (_, parsed_home_id) = parse_fips_home_id(home_id.as_bytes()).unwrap();
        let (_, parsed_workplace_id) = parse_fips_workplace_id(workplace_id.as_bytes()).unwrap();
        let (_, parsed_public_school_id) =
            parse_fips_school_id(public_school_id.as_bytes()).unwrap();
        let (_, parsed_private_school_id) =
            parse_fips_school_id(private_school_id.as_bytes()).unwrap();

        assert_eq!(home_id, format_as_fips_code_string(parsed_home_id));
        assert_eq!(
            workplace_id,
            format_as_fips_code_string(parsed_workplace_id)
        );
        assert_eq!(
            public_school_id,
            format_as_fips_code_string(parsed_public_school_id)
        );
        assert_eq!(
            private_school_id,
            format_as_fips_code_string(parsed_private_school_id)
        );
    }
}
