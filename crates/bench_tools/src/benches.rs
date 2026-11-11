// The benches are also a `[[bench]]` target, where everything is used; nothing in the lib calls
// them.
#![allow(dead_code)]

#[cfg(test)]
pub(crate) mod dummy_bench;
