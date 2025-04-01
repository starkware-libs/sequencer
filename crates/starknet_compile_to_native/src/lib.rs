//! A lib for compiling Sierra into Native.

// When cairo_native is not active, this crate just exposes the config.
pub mod config;

#[cfg(feature = "cairo_native")]
// Include the rest of the crate when cairo_native is active.
include!("lib_with_cairo_native.rs");
