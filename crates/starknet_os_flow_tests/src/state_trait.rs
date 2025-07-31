#![allow(dead_code)]
use blockifier::state::state_api::UpdatableState;

pub(crate) trait FlowTestState: UpdatableState + Default + Sync + Send + 'static {}
