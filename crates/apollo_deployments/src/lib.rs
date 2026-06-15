// TODO(Tsabary): reduce visibility when possible.
pub(crate) mod addresses;
pub mod deployment_definitions;
pub mod deployments;
pub mod jsonnet_generation;
#[cfg(test)]
pub mod jsonnet_test;
pub(crate) mod replacers;
pub(crate) mod scale_policy;
pub mod service;
#[cfg(test)]
pub mod test_utils;
pub(crate) mod utils;
