#[cfg(not(feature = "cairo_native"))]
fn main() {
    // Cairo Native is not enabled, so we don't need to do anything.
}

#[cfg(feature = "cairo_native")]
include!("build_with_cairo_native.rs");
