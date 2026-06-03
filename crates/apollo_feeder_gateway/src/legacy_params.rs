//! Replicates how the legacy Python feeder gateway parses numeric query values:
//! `json.loads(value)` followed by per-endpoint integer handling, including the exact live error
//! messages (verified 2026-06-03). Documented divergences from Python, accepted as out of scope:
//! JSON strings/arrays/objects as values (live coerces or type-errors on them; here they fall to
//! the `Expecting value` message) and scientific-notation float echoes (Python's shortest-repr
//! formatting differs from Rust's).

use num_traits::ToPrimitive;

#[cfg(test)]
#[path = "legacy_params_test.rs"]
mod legacy_params_test;

/// A scalar parsed the way Python's `json.loads` parses a legacy numeric query value.
#[derive(Debug, PartialEq)]
pub(crate) enum LegacyJsonScalar {
    /// A JSON integer. `python_repr` is the digits Python echoes (arbitrary precision); `value`
    /// is its `u64` form, `None` when negative or beyond `u64` (no such block can exist).
    Int {
        python_repr: String,
        negative: bool,
        value: Option<u64>,
    },
    /// A JSON float; `truncated` is Python's `int()` coercion (toward zero).
    Float {
        python_repr: String,
        truncated: i64,
    },
    Bool(bool),
    Null,
}

/// Parses `raw` as Python's `json.loads` would parse a scalar; the error is the exact live
/// MALFORMED_REQUEST message (a `json.loads` error echo).
pub(crate) fn parse_legacy_json_scalar(raw: &str) -> Result<LegacyJsonScalar, String> {
    const EXPECTING_VALUE: &str = "Expecting value: line 1 column 1 (char 0)";
    match raw {
        "null" => return Ok(LegacyJsonScalar::Null),
        "true" => return Ok(LegacyJsonScalar::Bool(true)),
        "false" => return Ok(LegacyJsonScalar::Bool(false)),
        _ => {}
    }

    let number_prefix_length = json_number_prefix_length(raw);
    if number_prefix_length == 0 {
        return Err(EXPECTING_VALUE.to_string());
    }
    if number_prefix_length < raw.len() {
        // json.loads parses the leading number and reports the rest, 1-based for the column.
        return Err(format!(
            "Extra data: line 1 column {} (char {})",
            number_prefix_length + 1,
            number_prefix_length
        ));
    }

    if raw.contains(['.', 'e', 'E']) {
        let parsed_float: f64 = raw.parse().map_err(|_| EXPECTING_VALUE.to_string())?;
        // Truncation toward zero matches Python's int(); a float beyond i64 saturates, which is
        // out of block-number range regardless.
        let truncated = parsed_float
            .trunc()
            .to_i64()
            .unwrap_or(if parsed_float.is_sign_negative() { i64::MIN } else { i64::MAX });
        return Ok(LegacyJsonScalar::Float {
            python_repr: format_python_float(parsed_float),
            truncated,
        });
    }

    let negative = raw.starts_with('-');
    let value = if negative { None } else { raw.parse::<u64>().ok() };
    Ok(LegacyJsonScalar::Int { python_repr: raw.to_string(), negative, value })
}

/// The length of the longest JSON-number prefix of `raw` (JSON grammar: optional `-`, then `0` or
/// a non-zero-led digit run, then optional fraction and exponent), or 0 if none.
fn json_number_prefix_length(raw: &str) -> usize {
    let bytes = raw.as_bytes();
    let mut index = 0;
    if bytes.get(index) == Some(&b'-') {
        index += 1;
    }
    match bytes.get(index) {
        // JSON forbids leading zeros: `01` parses as `0` with trailing data.
        Some(b'0') => index += 1,
        Some(digit) if digit.is_ascii_digit() => {
            while bytes.get(index).is_some_and(|byte| byte.is_ascii_digit()) {
                index += 1;
            }
        }
        _ => return 0,
    }
    if bytes.get(index) == Some(&b'.')
        && bytes.get(index + 1).is_some_and(|byte| byte.is_ascii_digit())
    {
        index += 1;
        while bytes.get(index).is_some_and(|byte| byte.is_ascii_digit()) {
            index += 1;
        }
    }
    if matches!(bytes.get(index), Some(b'e' | b'E')) {
        let mut exponent_index = index + 1;
        if matches!(bytes.get(exponent_index), Some(b'+' | b'-')) {
            exponent_index += 1;
        }
        if bytes.get(exponent_index).is_some_and(|byte| byte.is_ascii_digit()) {
            while bytes.get(exponent_index).is_some_and(|byte| byte.is_ascii_digit()) {
                exponent_index += 1;
            }
            index = exponent_index;
        }
    }
    index
}

/// Formats a float the way Python's repr does for plain decimal values (e.g. `1.5`, `2.0`).
fn format_python_float(value: f64) -> String {
    let formatted = format!("{value}");
    if formatted.contains('.') { formatted } else { format!("{formatted}.0") }
}
