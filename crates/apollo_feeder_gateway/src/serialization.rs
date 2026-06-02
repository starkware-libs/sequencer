use std::io;

use axum::http::header;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::ser::{Formatter, Serializer};

use crate::errors::FeederGatewayError;
use crate::reader::FgResult;

#[cfg(test)]
#[path = "serialization_test.rs"]
mod serialization_test;

/// A `serde_json` formatter that reproduces Python's `json.dumps` default output, so the feeder
/// gateway is byte-identical to the legacy Python feeder gateway (backwards compatibility requires
/// matching key order, separators, and escaping exactly):
///
/// - **Spaced separators**: `", "` between elements and `": "` after keys (serde's default is
///   compact, `,`/`:`), on a single line with no indentation.
/// - **`ensure_ascii=True`**: every non-ASCII scalar is escaped as `\uXXXX` (UTF-16 code units,
///   surrogate pairs above the BMP), matching Python.
///
/// Everything else (number formatting, the standard `"`/`\`/control-character escapes) delegates to
/// `serde_json`'s default `Formatter` methods.
#[derive(Clone, Copy, Debug, Default)]
struct PythonFormatter;

impl Formatter for PythonFormatter {
    fn begin_array_value<W: ?Sized + io::Write>(
        &mut self,
        writer: &mut W,
        first: bool,
    ) -> io::Result<()> {
        if first { Ok(()) } else { writer.write_all(b", ") }
    }

    fn begin_object_key<W: ?Sized + io::Write>(
        &mut self,
        writer: &mut W,
        first: bool,
    ) -> io::Result<()> {
        if first { Ok(()) } else { writer.write_all(b", ") }
    }

    fn begin_object_value<W: ?Sized + io::Write>(&mut self, writer: &mut W) -> io::Result<()> {
        writer.write_all(b": ")
    }

    fn write_string_fragment<W: ?Sized + io::Write>(
        &mut self,
        writer: &mut W,
        fragment: &str,
    ) -> io::Result<()> {
        // `serde_json` has already routed quotes, backslashes, and control characters through
        // `write_char_escape`, so this fragment only needs ASCII-escaping of the remaining
        // non-ASCII scalars.
        let mut ascii_run_start = 0;
        for (index, character) in fragment.char_indices() {
            if character.is_ascii() {
                continue;
            }
            if ascii_run_start < index {
                writer.write_all(&fragment.as_bytes()[ascii_run_start..index])?;
            }
            let mut utf16_buffer = [0u16; 2];
            for code_unit in character.encode_utf16(&mut utf16_buffer) {
                write!(writer, "\\u{code_unit:04x}")?;
            }
            ascii_run_start = index + character.len_utf8();
        }
        if ascii_run_start < fragment.len() {
            writer.write_all(&fragment.as_bytes()[ascii_run_start..])?;
        }
        Ok(())
    }
}

/// Serializes `value` to a JSON string byte-identical to Python's `json.dumps` default output (see
/// [`PythonFormatter`]). Feeder gateway handlers MUST serialize responses through this function
/// rather than `serde_json::to_string`/`to_vec` or axum `Json<T>` (those are compact and break byte
/// parity).
pub fn to_python_json<T: Serialize>(value: &T) -> FgResult<String> {
    let mut buffer = Vec::new();
    let mut serializer = Serializer::with_formatter(&mut buffer, PythonFormatter);
    value.serialize(&mut serializer).map_err(|error| {
        tracing::error!(error = %error, "feeder gateway JSON serialization failed");
        FeederGatewayError::Internal
    })?;
    String::from_utf8(buffer).map_err(|error| {
        tracing::error!(error = %error, "feeder gateway JSON serialization produced invalid UTF-8");
        FeederGatewayError::Internal
    })
}

/// Builds a feeder gateway JSON `Response` from `value`, serialized with the byte-parity
/// [`to_python_json`] formatter (never axum `Json<T>`, which is compact). On the unexpected
/// serialization failure, returns the internal-error envelope.
pub fn fg_json<T: Serialize>(value: &T) -> Response {
    match to_python_json(value) {
        Ok(body) => ([(header::CONTENT_TYPE, "application/json")], body).into_response(),
        Err(error) => error.into_response(),
    }
}
