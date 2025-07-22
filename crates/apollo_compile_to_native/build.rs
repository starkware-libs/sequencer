#[cfg(not(feature = "cairo_native"))]
fn main() {
    // Cairo Native is not enabled, so we don't need to do anything.
    // However, we still need to tell Cargo when to rerun this script.
    // Since this script does nothing, we can tell it to never rerun by
    // only watching for changes to a file that will never change.
    println!("cargo:rerun-if-changed=build.rs");
}

#[cfg(feature = "cairo_native")]
include!("build_with_cairo_native.rs");
