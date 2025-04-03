//! A lib for compiling Sierra into Native.

// When cairo_native is not active, this crate just exposes the config.
pub mod config;

// Include the rest of the crate when cairo_native is active.
#[cfg(feature = "cairo_native")]
pub mod compiler;
#[cfg(feature = "cairo_native")]
pub mod constants;

#[cfg(all(feature = "cairo_native", test))]
#[path = "compile_test.rs"]
pub mod compile_test;

#[cfg(all(feature = "cairo_native", test))]
#[path = "constants_test.rs"]
pub mod constants_test;
