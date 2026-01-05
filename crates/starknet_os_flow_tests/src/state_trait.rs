use blockifier::state::state_api::UpdatableState;
use blockifier::test_utils::dict_state_reader::DictStateReader;

pub(crate) trait FlowTestState: Clone + UpdatableState + Send {
    fn create_empty_state() -> Self;
}

impl FlowTestState for DictStateReader {
    fn create_empty_state() -> Self {
        DictStateReader::default()
    }
}
