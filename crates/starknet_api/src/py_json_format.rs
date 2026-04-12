//! JSON serialization matching Python `json.dumps()` separators (`, ` / `: `).

use serde::Serialize;

/// Formats a json object in the same way that python's json.dumps() formats.
pub(crate) struct PyJsonFormatter;

impl PyJsonFormatter {
    pub(crate) fn comma() -> &'static [u8; 2] {
        b", "
    }

    pub(crate) fn colon() -> &'static [u8; 2] {
        b": "
    }
}

impl serde_json::ser::Formatter for PyJsonFormatter {
    fn begin_array_value<W: ?Sized + std::io::Write>(
        &mut self,
        writer: &mut W,
        first: bool,
    ) -> std::io::Result<()> {
        if !first {
            writer.write_all(Self::comma())?;
        }
        Ok(())
    }

    fn begin_object_key<W: ?Sized + std::io::Write>(
        &mut self,
        writer: &mut W,
        first: bool,
    ) -> std::io::Result<()> {
        if !first {
            writer.write_all(Self::comma())?;
        }
        Ok(())
    }

    fn begin_object_value<W: ?Sized + std::io::Write>(
        &mut self,
        writer: &mut W,
    ) -> std::io::Result<()> {
        writer.write_all(Self::colon())
    }
}

pub(crate) fn py_json_dumps<T: ?Sized + Serialize>(value: &T) -> Result<String, serde_json::Error> {
    let mut string_buffer = vec![];
    let mut ser = serde_json::Serializer::with_formatter(&mut string_buffer, PyJsonFormatter);
    value.serialize(&mut ser)?;
    Ok(String::from_utf8(string_buffer).expect("serialized JSON should be valid UTF-8"))
}
