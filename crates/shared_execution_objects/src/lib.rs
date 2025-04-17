//! The `shared_execution_objects` crate contains execution objects shared between different crates.

/// Contains a rust version of objects on the centralized Python side. These objects are used when
/// interacting with the centralized Python.
pub mod central_objects;
#[cfg(test)]
mod central_objects_test;
// TODO(Yael): add the execution objects from the blockifier: execution_info, bouncer_weights,
// commitment_state_diff.
