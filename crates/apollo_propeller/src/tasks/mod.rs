//! Per-message task implementations.

pub(crate) mod state_manager_task;
pub(crate) mod task_messages;
pub(crate) mod validator_task;

pub(crate) use state_manager_task::spawn_state_manager_task;
pub(crate) use task_messages::StateManagerToCore;
pub(crate) use validator_task::{spawn_validator_task, UnitToValidate};

#[cfg(test)]
mod state_manager_task_test;
