use std::{io::Error as IoError, path::PathBuf};
use thiserror::Error;

use super::{CountyCode, DataCode, IdCode, SettingCategoryCode, StateCode, TractCode};
use zip::result::ZipError;

#[derive(Debug, Error)]
pub enum PopulationReaderError {
    #[error("population reader IO error: {0}")]
    Io(#[source] IoError),
    #[error("population reader file error for {path:?}: {source}")]
    FileError {
        path: PathBuf,
        #[source]
        source: Box<PopulationReaderError>,
    },
    #[error("population reader parse error in {field_name} on line {line_number}: {source}")]
    Parse {
        field_name: &'static str,
        line_number: usize,
        #[source]
        source: FIPSParserError,
    },
    #[error(
        "population reader wrong field count on line {line_number}: expected {expected}, found {found}"
    )]
    WrongFieldCount {
        expected: usize,
        found: usize,
        line_number: usize,
    },
    #[error("population reader data file is empty: {0}")]
    EmptyFile(PathBuf),
    #[error("population reader zip error: {0}")]
    ZipError(#[source] ZipError),
}

/// The FIPS parser error type.
///
/// We assume the length of the input text is small enough that it's not necessary to attach source
/// location information, e.g. parsing a string containing a single code.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum FIPSParserError {
    #[error("invalid digit {found:?} at byte index {at_index}")]
    InvalidDigit {
        /// The non-digit character, if it decodes as valid UTF-8.
        found: Option<char>,
        /// The index of the non-digit character in the input string.
        at_index: usize,
    },
    #[error("expected {expected} digits, found {found}")]
    InvalidLength {
        /// The exact digit count required by the parser in this context.
        expected: usize,
        /// How many digits were actually available before the input ended.
        found: usize,
    },
    #[error("value {value_prefix} exceeds maximum {capacity}")]
    ValueExceedsCapacity {
        /// The parsed prefix that exceeds the bit-width constraint
        value_prefix: String,
        /// The largest value that can be represented in the requested bit width.
        capacity: u64,
    },
    #[error("value {value} is not a valid state code")]
    InvalidStateCode {
        /// The parsed numeric value
        value: StateCode,
    },
}

/// Similar to how Nom structures its results. We have:
///   `I`: The input type, i.e. `&str`
///   `O`: The output type, i.e. `u32`
///   `E`: The error type returns a tuple of the original input and the error.
/// A successful result consists of the remaining unparsed input and the parsed value.
pub type IResult<I, O, E = (I, FIPSParserError)> = Result<(I, O), E>;
pub type FIPSParseResult<'a, T> = IResult<&'a [u8], T>;

/// We only have one error case, namely when a value is out of range. Instances of `FIPSError` are
/// constructed with the `FIPSError::from_*_code()` constructor methods in the
/// `FIPSCode::encode_*()` constructor methods.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Error)]
#[error("value {value} provided for {parameter_name} is outside valid range of {min}..{max}")]
pub struct FIPSError {
    parameter_name: &'static str,
    value: u64,
    min: u64,
    max: u64,
}

impl FIPSError {
    #[must_use]
    pub fn new(parameter_name: &'static str, value: u64, min: u64, max: u64) -> Self {
        Self {
            parameter_name,
            value,
            min,
            max,
        }
    }

    // Convenience constructors for the code types. These should be kept in sync with the module-level documentation
    // in `fips_code.rs`.

    /// This one is unique in that it represents an error converting a (presumably valid) [`StateCode`] to a [`USState`]
    /// variant. We lie a little bit and claim that values in 1..57 are valid when 3, 7, 14, 43, and 52 are not.
    #[must_use]
    pub fn from_us_state(value: StateCode) -> Self {
        Self {
            parameter_name: "USState Code",
            value: value as u64,
            min: 1,
            max: 57, // 1..57
        }
    }

    #[must_use]
    pub fn from_state_code(value: StateCode) -> Self {
        Self {
            parameter_name: "StateCode",
            value: value as u64,
            min: 1,
            max: 100, // Two decimal digits
        }
    }

    #[must_use]
    pub fn from_county_code(value: CountyCode) -> Self {
        Self {
            parameter_name: "CountyCode",
            value: value as u64,
            min: 0,
            max: 1000, // Three decimal digits
        }
    }

    #[must_use]
    pub fn from_tract_code(value: TractCode) -> Self {
        Self {
            parameter_name: "TractCode",
            value: value as u64,
            min: 0,
            max: 1_000_000, // Six decimal digits
        }
    }

    #[must_use]
    pub fn from_setting_category_code(value: SettingCategoryCode) -> Self {
        Self {
            parameter_name: "SettingCategoryCode",
            value: value as u64,
            min: 0,
            max: 16, // 2^4
        }
    }

    #[must_use]
    pub fn from_id_code(value: IdCode) -> Self {
        Self {
            parameter_name: "IdCode",
            value: value as u64,
            min: 0,
            max: 32_768, // 2^15
        }
    }

    #[must_use]
    pub fn from_data_code(value: DataCode) -> Self {
        Self {
            parameter_name: "DataCode",
            value: value as u64,
            min: 0,
            max: 256, // 2^8
        }
    }
}
