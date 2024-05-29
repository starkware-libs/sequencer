//! A lib for compiling Sierra into Casm.

pub mod compile;

#[cfg(any(feature = "testing", test))]
pub mod test_utils;
