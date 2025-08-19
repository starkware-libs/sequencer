//! A lib for compiling Sierra into Native.

// Include the compilation modules
pub mod compiler;
pub mod constants;

#[cfg(test)]
#[path = "compile_test.rs"]
pub mod compile_test;

#[cfg(test)]
#[path = "constants_test.rs"]
pub mod constants_test;
