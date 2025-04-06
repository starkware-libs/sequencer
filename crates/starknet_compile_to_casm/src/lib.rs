//! A lib for compiling Sierra into Casm.

pub mod compiler;
pub mod config;
pub mod constants;

#[cfg(test)]
#[path = "compile_test.rs"]
pub mod compile_test;
