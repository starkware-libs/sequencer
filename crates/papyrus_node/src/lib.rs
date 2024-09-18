// config compiler to support coverage_attribute feature when running coverage in nightly mode
// within this crate
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

#[cfg(any(test, feature = "testing"))]
pub mod bin_utils;
#[allow(unused_imports)]
pub mod config;
#[cfg(test)]
mod precision_test;
pub mod run;
pub mod version;
