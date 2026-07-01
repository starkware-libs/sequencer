// TODO(Tsabary): reduce visibility when possible.
pub(crate) mod addresses;
pub mod deployment_definitions;
pub mod deployments;
#[cfg(any(feature = "testing", test))]
pub mod jsonnet_eval;
#[cfg(test)]
mod jsonnet_tests;
pub(crate) mod replacers;
pub(crate) mod scale_policy;
pub mod service;
#[cfg(test)]
pub mod test_utils;
pub(crate) mod utils;
