use blockifier::test_utils::dict_state_reader::DictStateReader;
use tokio::test;

use crate::initial_state::create_default_initial_state_data;

#[test]
async fn test_initial_state_creation() {
    let _initial_state = create_default_initial_state_data::<DictStateReader>().await;
}
