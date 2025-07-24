use std::collections::HashMap;

use cairo_vm::serde::deserialize_program::Location;
use cairo_vm::vm::errors::vm_exception::{get_error_attr_value, get_location};
use cairo_vm::vm::runners::cairo_runner::CairoRunner;

use crate::CAIRO_FILES_MAP;

#[cfg(test)]
#[path = "os_code_snippets_test.rs"]
pub mod test;

fn get_code_snippet(location: &Location) -> Option<String> {
    get_code_snippet_from_filemap(location, &CAIRO_FILES_MAP)
}

/// Gets a code snippet from an OS file at a specific location.
fn get_code_snippet_from_filemap(
    location: &Location,
    files_map: &HashMap<String, String>,
) -> Option<String> {
    let path = location.input_file.filename.split_once("cairo/").map(|(_, rest)| rest)?;
    let file_bytes = files_map.get(path)?.as_bytes();

    Some(location.get_location_marks(file_bytes).to_string())
}

/// Adds code snippets to the traceback of a VM exception.
#[allow(dead_code)]
pub fn get_traceback_with_code_snippet(runner: &CairoRunner) -> Option<String> {
    let traceback: String = runner
        .vm
        .get_traceback_entries()
        .iter()
        .map(|(_fp, traceback_pc)| {
            let unknown_frame = format!("Unknown location (pc={traceback_pc})\n");
            if traceback_pc.segment_index != 0 {
                return unknown_frame;
            }
            if let Some(ref attr) = get_error_attr_value(traceback_pc.offset, runner) {
                return format!("{attr}\n");
            }
            match &get_location(traceback_pc.offset, runner, None) {
                Some(location) => {
                    let location_str = location.to_string(&format!("(pc={traceback_pc})"));
                    let snippet = get_code_snippet(location)
                        .unwrap_or_else(|| "Code snippet not found.".to_string());
                    format!("{location_str}\n{snippet}\n")
                }
                None => unknown_frame,
            }
        })
        .collect();

    (!traceback.is_empty())
        .then(|| format!("Cairo traceback (most recent call last):\n{traceback}"))
}
