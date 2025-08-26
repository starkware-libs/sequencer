<<<<<<< HEAD
#![allow(dead_code)]
use blockifier::state::state_api::UpdatableState;
use blockifier::test_utils::dict_state_reader::DictStateReader;

pub(crate) trait FlowTestState: UpdatableState + Sync + Send + 'static {
    fn create_empty_state() -> Self;
}

impl FlowTestState for DictStateReader {
    fn create_empty_state() -> Self {
        DictStateReader::default()
    }
}
||||||| 01792faa8
=======
#![allow(dead_code)]
use blockifier::state::state_api::UpdatableState;

pub(crate) trait FlowTestState: UpdatableState + Default + Sync + Send + 'static {}
>>>>>>> origin/main-v0.14.1
