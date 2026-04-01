#[cfg(test)]
#[path = "cairo_compiler_version_test.rs"]
mod cairo_compiler_version_test;

pub const CAIRO1_COMPILER_VERSION: &str =
    include_str!("cairo_compiler_version.txt").trim_ascii_end();
