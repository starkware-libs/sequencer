use std::collections::HashMap;

use cairo_vm::serde::deserialize_program::Location;

#[cfg(test)]
#[path = "os_code_snippets_test.rs"]
pub mod test;

#[allow(dead_code)]
/// Gets a code snippet from an OS file at a specific location.
fn get_code_snippet(location: &Location, files_map: &HashMap<String, String>) -> Option<String> {
    let path = location.input_file.filename.split_once("cairo/").map(|(_, rest)| rest)?;
    let file_bytes = files_map.get(path)?.as_bytes();

    Some(location.get_location_marks(file_bytes).to_string())
}
