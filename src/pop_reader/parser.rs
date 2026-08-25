#![allow(mismatched_lifetime_syntaxes)]
//! These high-level functions parse the concatenated FIPS code and ids.

use super::{
    CountyCode, FIPSCode, IdCode, PopulationReaderSettingCategory, StateCode, TractCode,
    errors::FIPSParserError, fips_code::FIFTEEN_BIT_MASK,
};

/// Parser result used throughout this module.
///
/// On success, returns `(remaining_input, parsed_value)`. On failure,
/// returns `(original_input, error)`, where `error` is a [`FIPSParserError`].
pub type FIPSParseResult<'a, T> = Result<(&'a [u8], T), (&'a [u8], FIPSParserError)>;

/// Parses a fixed number of ASCII decimal digits from `input`, enforcing that the parsed value
/// fits into `bit_count` bits.
///
/// This mirrors `ixa_fips::parser::parse_decimal_digits_to_bits`, but operates on `&[u8]`
/// instead of `&str` to avoid UTF-8/string handling on CSV byte slices.
pub fn parse_decimal_digits_to_bits(
    digit_count: usize,
    bit_count: u8,
    input: &[u8],
) -> FIPSParseResult<'_, u64> {
    let maximum_allowed_value = if bit_count >= 64 {
        // Shifting by >= 64 bits is undefined behavior
        u64::MAX
    } else {
        (1u64 << bit_count) - 1
    };
    let mut input_bytes = input.iter();
    let mut computed_value: u64 = 0;

    for idx in 0..digit_count {
        match input_bytes.next() {
            Some(c) => {
                if c.is_ascii_digit() {
                    let digit = (c - b'0') as u64;

                    // Enforce the bit count constraint.
                    if computed_value > maximum_allowed_value / 10
                        || (computed_value == maximum_allowed_value / 10
                            && digit > maximum_allowed_value % 10)
                    {
                        return Err((
                            input,
                            FIPSParserError::ValueExceedsCapacity {
                                value_prefix: std::str::from_utf8(&input[..=idx])
                                    .unwrap()
                                    .to_owned(),
                                capacity: maximum_allowed_value,
                            },
                        ));
                    }

                    computed_value = computed_value * 10 + digit;
                } else {
                    // Try to decode the offending character as UTF-8.
                    // The UTF-8 encoded character at `idx` might not be represented as a single byte.
                    // However, as we assume ASCII decimal digits, we are guaranteed that the first
                    // `idx-1` bytes represent `idx-1` characters.
                    let found = std::str::from_utf8(&input[idx..])
                        .ok()
                        .and_then(|s| s.chars().next());

                    return Err((
                        input,
                        FIPSParserError::InvalidDigit {
                            found,
                            at_index: idx,
                        },
                    ));
                }
            }

            None => {
                // Ran out of digits before we were done parsing.
                return Err((
                    input,
                    FIPSParserError::InvalidLength {
                        expected: digit_count,
                        found: idx,
                    },
                ));
            }
        } // end match next byte
    } // end for idx

    let remaining = &input[digit_count..];
    Ok((remaining, computed_value))
}

/// Parses the first two decimal digits of `input` into a [`StateCode`].
fn parse_state_code(input: &[u8]) -> FIPSParseResult<StateCode> {
    parse_decimal_digits_to_bits(2, 7, input).map(|(rest, value)| (rest, value as StateCode))
}

/// Parses the input as a FIPS code for a home id. Returns `(rest, FIPSCode)`,
/// where `rest` is the remaining input after the FIPS code.
pub fn parse_fips_home_id(input: &[u8]) -> FIPSParseResult<FIPSCode> {
    let (rest, state): (&[u8], StateCode) = parse_state_code(input)?;
    let (rest, county): (&[u8], CountyCode) = parse_county_code(rest)?;
    let (rest, tract): (&[u8], TractCode) = parse_tract_code(rest)?;
    let (rest, home_id): (&[u8], IdCode) = parse_home_id(rest)?;

    // Because the parser functions verify that the parsed values fit into the required number of bits,
    // this should be infallible unless the parser and `FIPSCode` constructor invariants drift apart.
    let fips_code = FIPSCode::new(
        state,
        county,
        tract,
        PopulationReaderSettingCategory::Home.encode(),
        home_id,
        0,
    );
    match fips_code {
        Ok(fips_code) => Ok((rest, fips_code)),
        Err(_) => {
            panic!("FIPS code is invalid. This is a bug in the population ID parser.");
        }
    }
}

/// Parses the input as a FIPS code for a school id. Returns `(rest, FIPSCode)`,
/// where `rest` is the remaining input after the FIPS code.
pub fn parse_fips_school_id(input: &[u8]) -> FIPSParseResult<FIPSCode> {
    let (rest, state): (&[u8], StateCode) = parse_state_code(input)?;
    let (rest, county): (&[u8], CountyCode) = parse_county_code(rest)?;

    if rest.starts_with(b"x") {
        // Private school id
        let (rest, school_id): (&[u8], IdCode) = parse_private_school_id(rest)?;
        let fips_code = FIPSCode::new(
            state,
            county,
            0,
            PopulationReaderSettingCategory::PrivateSchool.encode(),
            school_id,
            0,
        );
        match fips_code {
            Ok(fips_code) => Ok((rest, fips_code)),
            Err(_) => {
                panic!("FIPS code is invalid. This is a bug in the population ID parser.");
            }
        }
    } else {
        // Public school
        // Public schools also have a tract code.
        let (rest, tract): (&[u8], TractCode) = parse_tract_code(rest)?;
        let (rest, school_id): (&[u8], IdCode) = parse_public_school_id(rest)?;
        let fips_code = FIPSCode::new(
            state,
            county,
            tract,
            PopulationReaderSettingCategory::PublicSchool.encode(),
            school_id,
            0,
        );
        match fips_code {
            Ok(fips_code) => Ok((rest, fips_code)),
            Err(_) => {
                panic!("FIPS code is invalid. This is a bug in the population ID parser.");
            }
        }
    }
}

/// Parses the input as a FIPS code for a workplace id. Returns `(rest, FIPSCode)`,
/// where `rest` is the remaining input after the FIPS code.
pub fn parse_fips_workplace_id(input: &[u8]) -> FIPSParseResult<FIPSCode> {
    let (rest, state): (&[u8], StateCode) = parse_state_code(input)?;
    let (rest, county): (&[u8], CountyCode) = parse_county_code(rest)?;
    let (rest, tract): (&[u8], TractCode) = parse_tract_code(rest)?;
    let (rest, workplace_id): (&[u8], IdCode) = parse_workplace_id(rest)?;

    let fips_code = FIPSCode::new(
        state,
        county,
        tract,
        PopulationReaderSettingCategory::Workplace.encode(),
        workplace_id,
        0,
    );
    match fips_code {
        Ok(fips_code) => Ok((rest, fips_code)),
        Err(_) => {
            panic!("FIPS code is invalid. This is a bug in the population ID parser.");
        }
    }
}

/// Parses the input as a FIPS code for a state + county id. Returns `(rest, FIPSCode)`,
/// where `rest` is the remaining input after the FIPS code.
pub fn parse_fips_state_county_id(input: &[u8]) -> FIPSParseResult<FIPSCode> {
    let (rest, state): (&[u8], StateCode) = parse_state_code(input)?;
    let (rest, county): (&[u8], CountyCode) = parse_county_code(rest)?;

    let fips_code = FIPSCode::new(
        state,
        county,
        0,
        PopulationReaderSettingCategory::Unspecified.encode(),
        0,
        0,
    );
    match fips_code {
        Ok(fips_code) => Ok((rest, fips_code)),
        Err(_) => {
            panic!("FIPS code is invalid. This is a bug in the population ID parser.");
        }
    }
}

/// Parses the first three digits of `input` as a county code. Enforces the requirement that the
/// value is representable using 10 bits (which is tautologically always true).
fn parse_county_code(input: &[u8]) -> FIPSParseResult<CountyCode> {
    parse_decimal_digits_to_bits(3, 10, input).map(|(rest, value)| (rest, value as CountyCode))
}

/// Parses the first six digits of `input` as a tract code. Enforces the requirement that the value
/// is representable using 20 bits (which is tautologically always true).
fn parse_tract_code(input: &[u8]) -> FIPSParseResult<TractCode> {
    parse_decimal_digits_to_bits(6, 20, input).map(|(rest, value)| (rest, value as TractCode))
}

/// Parses the next sequence of decimal digits in `input` as a setting id and verifies that the
/// parsed value fits in the 15-bit `FIPSCode` id field. Unlike many other `parse_*` functions,
/// this function consumes all remaining decimal digits of the input.
// ToDo: Is it an error if we do not parse the minimum number of digits described in the description
//       for this population format? Right now: an empty string is an error, but any nonempty string of
//       digits is allowed as long as it fits in the 15-bit id field.
fn parse_setting_id(input: &[u8]) -> FIPSParseResult<IdCode> {
    let (rest, value) = parse_integer(input)?;

    if value <= u64::from(FIFTEEN_BIT_MASK) {
        Ok((rest, value as IdCode))
    } else {
        let parsed_len = input.len() - rest.len();
        Err((
            input,
            FIPSParserError::ValueExceedsCapacity {
                value_prefix: std::str::from_utf8(&input[..parsed_len])
                    .unwrap()
                    .to_owned(),
                capacity: u64::from(FIFTEEN_BIT_MASK),
            },
        ))
    }
}

/// Parses the next sequence of decimal digits in `input` as a home id and verifies that the value
/// fits in the 15-bit `FIPSCode` id field.
fn parse_home_id(input: &[u8]) -> FIPSParseResult<IdCode> {
    parse_setting_id(input)
}

/// Parses the next sequence of decimal digits in `input` as a private-school id after stripping
/// `"xprvx"`, if it exists, and verifies that the value fits in the 15-bit `FIPSCode` id field.
fn parse_private_school_id(input: &[u8]) -> FIPSParseResult<IdCode> {
    let input = input.strip_prefix(b"xprvx").unwrap_or(input);
    parse_setting_id(input)
}

/// Parses the next sequence of decimal digits in `input` as a public-school id and verifies that
/// the value fits in the 15-bit `FIPSCode` id field.
fn parse_public_school_id(input: &[u8]) -> FIPSParseResult<IdCode> {
    parse_setting_id(input)
}

/// Parses the next sequence of decimal digits in `input` as a workplace id and verifies that the
/// value fits in the 15-bit `FIPSCode` id field.
fn parse_workplace_id(input: &[u8]) -> FIPSParseResult<IdCode> {
    parse_setting_id(input)
}

/// Parses the next sequence of decimal digits in `input` without respect to its length or how many
/// bits are required to represent it (though it must implicitly be at most 64).
pub fn parse_integer(input: &[u8]) -> FIPSParseResult<u64> {
    let mut computed_value = 0u64;
    const MAXIMUM_ALLOWED_VALUE: u64 = u64::MAX;
    let mut idx = 0usize;

    // Find the first non-digit character while accumulating the value.
    for &c in input {
        if c.is_ascii_digit() {
            let digit = u64::from(c - b'0');

            if computed_value > MAXIMUM_ALLOWED_VALUE / 10
                || (computed_value == MAXIMUM_ALLOWED_VALUE / 10
                    && digit > MAXIMUM_ALLOWED_VALUE % 10)
            {
                return Err((
                    input,
                    FIPSParserError::ValueExceedsCapacity {
                        value_prefix: std::str::from_utf8(&input[..=idx]).unwrap().to_owned(),
                        capacity: MAXIMUM_ALLOWED_VALUE,
                    },
                ));
            }

            computed_value = computed_value * 10 + digit;
            idx += 1;
        } else {
            break;
        }
    }

    if idx == 0 {
        return Err((
            input,
            FIPSParserError::InvalidLength {
                expected: 1,
                found: 0,
            },
        ));
    }

    Ok((&input[idx..], computed_value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pop_reader::{
        SettingCategoryCode, StateCode, fips_code::ExpandedFIPSCode, states::USState,
    };

    #[test]
    fn test_parse_home_id() {
        // Basic successful parsing
        assert_eq!(parse_home_id(b"1234rest"), Ok((&b"rest"[..], 1234)));
        assert_eq!(parse_home_id(b"0001xyz"), Ok((&b"xyz"[..], 1)));
        assert_eq!(parse_home_id(b"20507"), Ok((&b""[..], 20507)));

        // Maximum allowed value (15 bits max = 32767)
        assert_eq!(parse_home_id(b"32767abc"), Ok((&b"abc"[..], 32767)));

        // Edge cases
        assert_eq!(parse_home_id(b"0000test"), Ok((&b"test"[..], 0)));
        assert_eq!(parse_home_id(b"12"), Ok((&b""[..], 12)));

        // Error cases
        assert!(parse_home_id(b"").is_err()); // Empty string
        assert!(parse_home_id(b"abc").is_err()); // No digits
        assert_eq!(
            parse_home_id(b"32768"),
            Err((
                &b"32768"[..],
                FIPSParserError::ValueExceedsCapacity {
                    value_prefix: "32768".to_owned(),
                    capacity: u64::from(FIFTEEN_BIT_MASK),
                },
            ))
        );
    }

    #[test]
    fn test_parse_private_school_id() {
        // Basic successful parsing
        assert_eq!(
            parse_private_school_id(b"1234rest"),
            Ok((&b"rest"[..], 1234))
        );
        assert_eq!(parse_private_school_id(b"0001xyz"), Ok((&b"xyz"[..], 1)));

        // With 'xprvx' prefix
        assert_eq!(
            parse_private_school_id(b"xprvx1234rest"),
            Ok((&b"rest"[..], 1234))
        );
        assert_eq!(
            parse_private_school_id(b"xprvx1722xyz"),
            Ok((&b"xyz"[..], 1722))
        );

        // Maximum allowed value (15 bits max = 32767)
        assert_eq!(parse_private_school_id(b"32767"), Ok((&b""[..], 32767)));
        assert_eq!(
            parse_private_school_id(b"xprvx32767"),
            Ok((&b""[..], 32767))
        );

        // Edge cases
        assert_eq!(parse_private_school_id(b"0000test"), Ok((&b"test"[..], 0)));
        assert_eq!(
            parse_private_school_id(b"xprvx0000test"),
            Ok((&b"test"[..], 0))
        );

        // Error cases
        assert!(parse_private_school_id(b"").is_err()); // Empty string
        assert!(parse_private_school_id(b"xprvx").is_err()); // Empty after prefix
        assert!(parse_private_school_id(b"xprvxabc").is_err()); // No digits after prefix
        assert_eq!(
            parse_private_school_id(b"32768"),
            Err((
                &b"32768"[..],
                FIPSParserError::ValueExceedsCapacity {
                    value_prefix: "32768".to_owned(),
                    capacity: u64::from(FIFTEEN_BIT_MASK),
                },
            ))
        );
        assert_eq!(
            parse_private_school_id(b"xprvx32768"),
            Err((
                &b"32768"[..],
                FIPSParserError::ValueExceedsCapacity {
                    value_prefix: "32768".to_owned(),
                    capacity: u64::from(FIFTEEN_BIT_MASK),
                },
            ))
        );
    }

    #[test]
    fn test_parse_public_school_id() {
        // Basic successful parsing
        assert_eq!(parse_public_school_id(b"123rest"), Ok((&b"rest"[..], 123)));
        assert_eq!(parse_public_school_id(b"001xyz"), Ok((&b"xyz"[..], 1)));
        assert_eq!(parse_public_school_id(b"2789"), Ok((&b""[..], 2789)));

        // Maximum allowed value (15 bits max = 32767)
        assert_eq!(
            parse_public_school_id(b"32767abc"),
            Ok((&b"abc"[..], 32767))
        );

        // Edge cases
        assert_eq!(parse_public_school_id(b"000test"), Ok((&b"test"[..], 0)));
        assert_eq!(parse_public_school_id(b"12"), Ok((&b""[..], 12)));

        // Error cases
        assert!(parse_public_school_id(b"").is_err()); // Empty string
        assert!(parse_public_school_id(b"abc").is_err()); // No digits
        assert_eq!(
            parse_public_school_id(b"32768"),
            Err((
                &b"32768"[..],
                FIPSParserError::ValueExceedsCapacity {
                    value_prefix: "32768".to_owned(),
                    capacity: u64::from(FIFTEEN_BIT_MASK),
                },
            ))
        );
    }

    #[test]
    fn test_parse_workplace_id() {
        // Basic successful parsing
        assert_eq!(parse_workplace_id(b"12345rest"), Ok((&b"rest"[..], 12345)));
        assert_eq!(parse_workplace_id(b"00001xyz"), Ok((&b"xyz"[..], 1)));
        assert_eq!(parse_workplace_id(b"14938"), Ok((&b""[..], 14938)));

        // Maximum allowed value (15 bits max = 32767)
        assert_eq!(parse_workplace_id(b"32767abc"), Ok((&b"abc"[..], 32767)));

        // Edge cases
        assert_eq!(parse_workplace_id(b"00000test"), Ok((&b"test"[..], 0)));
        assert_eq!(parse_workplace_id(b"1234"), Ok((&b""[..], 1234)));

        // Error cases
        assert!(parse_workplace_id(b"").is_err()); // Empty string
        assert!(parse_workplace_id(b"abc").is_err()); // No digits
        assert_eq!(
            parse_workplace_id(b"32768"),
            Err((
                &b"32768"[..],
                FIPSParserError::ValueExceedsCapacity {
                    value_prefix: "32768".to_owned(),
                    capacity: u64::from(FIFTEEN_BIT_MASK),
                },
            ))
        );
    }

    #[test]
    fn test_parse_state_county_id() {
        // Basic successful parsing
        assert_eq!(
            parse_fips_state_county_id(b"11001rest"),
            Ok((&b"rest"[..], FIPSCode::new(11, 1, 0, 0, 0, 0).unwrap()))
        );

        // Error cases
        assert!(parse_fips_state_county_id(b"").is_err()); // Empty string
        assert!(parse_fips_state_county_id(b"abc").is_err()); // No digits
    }

    #[test]
    fn test_fips_home_id() {
        let home_id = b"110010109000024";
        let state_code: StateCode = 11;
        let county_code: CountyCode = 1;
        let tract_code: TractCode = 10900;
        let home_id_code = 24;

        let (_, parsed_home_id) = parse_fips_home_id(home_id).unwrap();

        assert_eq!(parsed_home_id.state_code(), state_code);
        assert_eq!(parsed_home_id.county_code(), county_code);
        assert_eq!(parsed_home_id.census_tract_code(), tract_code);
        assert_eq!(parsed_home_id.id(), home_id_code);
    }

    // A real life edge case
    #[test]
    fn test_fips_home_id_edge_case() {
        let home_id = b"1600101040110000";
        let state_code: StateCode = USState::ID.into();
        let county_code: CountyCode = 1;
        let tract_code: TractCode = 10401;
        let setting_cat_code: SettingCategoryCode = PopulationReaderSettingCategory::Home.encode();
        let home_id_code = 10000;

        let (_, parsed_home_id) = parse_fips_home_id(home_id).unwrap();

        assert_eq!(parsed_home_id.state_code(), state_code);
        assert_eq!(parsed_home_id.county_code(), county_code);
        assert_eq!(parsed_home_id.census_tract_code(), tract_code);
        assert_eq!(parsed_home_id.category_code(), setting_cat_code);
        assert_eq!(parsed_home_id.id(), home_id_code);
    }

    #[test]
    fn test_fips_work_id() {
        let workplace_id = b"1100100620201546";
        let state_code: StateCode = 11;
        let county_code: CountyCode = 1;
        let tract_code: TractCode = 6202;
        let workplace_id_code = 1546;

        let (_, parsed_workplace_id) = parse_fips_workplace_id(workplace_id).unwrap();

        assert_eq!(parsed_workplace_id.state_code(), state_code);
        assert_eq!(parsed_workplace_id.county_code(), county_code);
        assert_eq!(parsed_workplace_id.census_tract_code(), tract_code);
        assert_eq!(parsed_workplace_id.id(), workplace_id_code);
    }

    #[test]
    fn test_fips_public_school_id() {
        let public_school_id = b"11001009810157";
        let state_code: StateCode = 11;
        let county_code: CountyCode = 1;
        let tract_code: TractCode = 9810;
        let public_school_id_code = 157;

        let (_, parsed_public_school_id) = parse_fips_school_id(public_school_id).unwrap();

        assert_eq!(parsed_public_school_id.state_code(), state_code);
        assert_eq!(parsed_public_school_id.county_code(), county_code);
        assert_eq!(parsed_public_school_id.census_tract_code(), tract_code);
        assert_eq!(parsed_public_school_id.id(), public_school_id_code);
    }

    #[test]
    fn test_fips_private_school_id() {
        let private_school_id = b"24031xprvx0150";
        let state_code: StateCode = 24;
        let county_code: CountyCode = 31;
        let tract_code: TractCode = 0;
        let private_school_id_code = 150;

        let (_, parsed_private_school_id) = parse_fips_school_id(private_school_id).unwrap();

        assert_eq!(parsed_private_school_id.state_code(), state_code);
        assert_eq!(parsed_private_school_id.county_code(), county_code);
        assert_eq!(parsed_private_school_id.census_tract_code(), tract_code);
        assert_eq!(parsed_private_school_id.id(), private_school_id_code);
    }

    #[test]
    fn test_fips_public_school_id_edge_case() {
        let public_school_id = b"160010104012789";
        let state_code: StateCode = USState::ID.into();
        let county_code: CountyCode = 1;
        let tract_code: TractCode = 10401;
        let setting_cat_code: SettingCategoryCode =
            PopulationReaderSettingCategory::PublicSchool.encode();
        let public_school_id_code = 2789;

        let (_, parsed_public_school_id) = parse_fips_school_id(public_school_id).unwrap();

        assert_eq!(parsed_public_school_id.state_code(), state_code);
        assert_eq!(parsed_public_school_id.county_code(), county_code);
        assert_eq!(parsed_public_school_id.census_tract_code(), tract_code);
        assert_eq!(parsed_public_school_id.category_code(), setting_cat_code);
        assert_eq!(parsed_public_school_id.id(), public_school_id_code);
    }

    #[test]
    fn test_fips_private_school_id_edge_case() {
        let private_school_id = b"24031xprvx1722";
        let state_code: StateCode = 24;
        let county_code: CountyCode = 31;
        let tract_code: TractCode = 0;
        let setting_cat_code: SettingCategoryCode =
            PopulationReaderSettingCategory::PrivateSchool.encode();
        let private_school_id_code = 1722;

        let (_, parsed_private_school_id) = parse_fips_school_id(private_school_id).unwrap();

        assert_eq!(parsed_private_school_id.state_code(), state_code);
        assert_eq!(parsed_private_school_id.county_code(), county_code);
        assert_eq!(parsed_private_school_id.census_tract_code(), tract_code);
        assert_eq!(parsed_private_school_id.category_code(), setting_cat_code);
        assert_eq!(parsed_private_school_id.id(), private_school_id_code);
    }

    #[test]
    fn test_fips_work_id_edge_case() {
        let workplace_id = b"1600101040114938";
        let state_code: StateCode = USState::ID.into();
        let county_code: CountyCode = 1;
        let tract_code: TractCode = 10401;
        let setting_cat_code: SettingCategoryCode =
            PopulationReaderSettingCategory::Workplace.encode();
        let workplace_id_code = 14938;

        let (_, parsed_workplace_id) = parse_fips_workplace_id(workplace_id).unwrap();

        assert_eq!(parsed_workplace_id.state_code(), state_code);
        assert_eq!(parsed_workplace_id.county_code(), county_code);
        assert_eq!(parsed_workplace_id.census_tract_code(), tract_code);
        assert_eq!(parsed_workplace_id.category_code(), setting_cat_code);
        assert_eq!(parsed_workplace_id.id(), workplace_id_code);
    }

    #[test]
    fn test_parse_integer() {
        // Basic successful parsing
        assert_eq!(parse_integer(b"123rest"), Ok((&b"rest"[..], 123)));
        assert_eq!(parse_integer(b"0xyz"), Ok((&b"xyz"[..], 0)));
        assert_eq!(parse_integer(b"9876543210"), Ok((&b""[..], 9876543210)));

        // Single digit
        assert_eq!(parse_integer(b"5abc"), Ok((&b"abc"[..], 5)));

        // Long number
        assert_eq!(
            parse_integer(b"18446744073709551615end"),
            Ok((&b"end"[..], 18446744073709551615))
        ); // u64 max

        // Error cases
        assert!(parse_integer(b"").is_err()); // Empty string
        assert!(parse_integer(b"abc").is_err()); // No digits
    }

    // Additional combined tests
    #[test]
    fn test_combined_scenarios() {
        // Test with leading zeros
        assert_eq!(parse_home_id(b"0123"), Ok((&b""[..], 123)));
        assert_eq!(parse_private_school_id(b"xprvx0042"), Ok((&b""[..], 42)));

        // `parse_integer` consumes all contiguous digits.
        assert_eq!(parse_public_school_id(b"12345"), Ok((&b""[..], 12345)));

        // Test with value equal to max
        assert_eq!(parse_workplace_id(b"32767@#$%"), Ok((&b"@#$%"[..], 32767)));

        // Test with value exceeding max
        assert_eq!(
            parse_workplace_id(b"32768@#$%"),
            Err((
                &b"32768@#$%"[..],
                FIPSParserError::ValueExceedsCapacity {
                    value_prefix: "32768".to_owned(),
                    capacity: u64::from(FIFTEEN_BIT_MASK),
                }
            ))
        );

        // Test with special characters after digits
        assert_eq!(parse_workplace_id(b"14938@#$%"), Ok((&b"@#$%"[..], 14938)));
    }

    #[test]
    fn test_parse_aspr_data() {
        let test_data = vec![
            (
                b"481559501000128".as_slice(),
                ExpandedFIPSCode {
                    state: USState::TX.into(),
                    county: 155,
                    tract: 950100,
                    category: 1,
                    id: 128,
                    data: 0,
                },
            ),
            (
                b"48155950100001".as_slice(),
                ExpandedFIPSCode {
                    state: USState::TX.into(),
                    county: 155,
                    tract: 950100,
                    category: 3,
                    id: 1,
                    data: 0,
                },
            ),
            (
                b"021300003000173".as_slice(),
                ExpandedFIPSCode {
                    state: USState::AK.into(),
                    county: 130,
                    tract: 300,
                    category: 1,
                    id: 173,
                    data: 0,
                },
            ),
            (
                b"02130000400002".as_slice(),
                ExpandedFIPSCode {
                    state: USState::AK.into(),
                    county: 130,
                    tract: 400,
                    category: 3,
                    id: 2,
                    data: 0,
                },
            ),
            (
                b"021300001000499".as_slice(),
                ExpandedFIPSCode {
                    state: USState::AK.into(),
                    county: 130,
                    tract: 100,
                    category: 1,
                    id: 499,
                    data: 0,
                },
            ),
            (
                b"484879507000440".as_slice(),
                ExpandedFIPSCode {
                    state: USState::TX.into(),
                    county: 487,
                    tract: 950700,
                    category: 1,
                    id: 440,
                    data: 0,
                },
            ),
            (
                b"4848795060000714".as_slice(),
                ExpandedFIPSCode {
                    state: USState::TX.into(),
                    county: 487,
                    tract: 950600,
                    category: 2,
                    id: 714,
                    data: 0,
                },
            ),
            (
                b"484879506001139".as_slice(),
                ExpandedFIPSCode {
                    state: USState::TX.into(),
                    county: 487,
                    tract: 950600,
                    category: 1,
                    id: 1139,
                    data: 0,
                },
            ),
            (
                b"484879506001457".as_slice(),
                ExpandedFIPSCode {
                    state: USState::TX.into(),
                    county: 487,
                    tract: 950600,
                    category: 1,
                    id: 1457,
                    data: 0,
                },
            ),
            (
                b"4848795050000091".as_slice(),
                ExpandedFIPSCode {
                    state: USState::TX.into(),
                    county: 487,
                    tract: 950500,
                    category: 2,
                    id: 91,
                    data: 0,
                },
            ),
            (
                b"021300003000687".as_slice(),
                ExpandedFIPSCode {
                    state: USState::AK.into(),
                    county: 130,
                    tract: 300,
                    category: 1,
                    id: 687,
                    data: 0,
                },
            ),
            (
                b"021300002001412".as_slice(),
                ExpandedFIPSCode {
                    state: USState::AK.into(),
                    county: 130,
                    tract: 200,
                    category: 1,
                    id: 1412,
                    data: 0,
                },
            ),
            (
                b"0213000020000291".as_slice(),
                ExpandedFIPSCode {
                    state: USState::AK.into(),
                    county: 130,
                    tract: 200,
                    category: 2,
                    id: 291,
                    data: 0,
                },
            ),
            (
                b"484879505000385".as_slice(),
                ExpandedFIPSCode {
                    state: USState::TX.into(),
                    county: 487,
                    tract: 950500,
                    category: 1,
                    id: 385,
                    data: 0,
                },
            ),
            (
                b"021300002001170".as_slice(),
                ExpandedFIPSCode {
                    state: USState::AK.into(),
                    county: 130,
                    tract: 200,
                    category: 1,
                    id: 1170,
                    data: 0,
                },
            ),
        ];

        for (fips_code, expected) in test_data {
            // These codes are context sensitive. We cheat by storing the `SettingCategory` in the expected value
            // and using that to parse the code.
            let result = match expected.category {
                1 => parse_fips_home_id(fips_code),
                2 => parse_fips_workplace_id(fips_code),
                3 | 4 => parse_fips_school_id(fips_code),
                _ => panic!("Invalid category"),
            };
            let result: (&[u8], FIPSCode) = result.unwrap_or_else(|_| {
                panic!("Failed to parse {}", String::from_utf8_lossy(fips_code))
            });
            assert_eq!(ExpandedFIPSCode::from_fips_code(result.1), expected)
        }
    }
}
