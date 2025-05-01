#[cfg(feature = "dump_source_files")]
use std::collections::HashMap;
#[cfg(feature = "dump_source_files")]
use std::sync::LazyLock;

#[cfg(test)]
mod constants_test;

#[cfg(feature = "dump_source_files")]
pub static CAIRO_FILES_MAP: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!(concat!(env!("OUT_DIR"), "/cairo_files_map.json")))
        .unwrap_or_else(|error| panic!("Failed to deserialize cairo_files_map.json: {error:?}."))
});
