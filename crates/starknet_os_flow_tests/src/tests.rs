use blockifier::test_utils::dict_state_reader::DictStateReader;
use tokio::test as tokio_test;

use crate::utils::{create_default_initial_state, flow_test_body, poc_txs, InitialStateData};

#[tokio_test]
async fn empty_run() {
    let initial_state: InitialStateData<DictStateReader> = InitialStateData::default();
    let txs = vec![];
    flow_test_body(initial_state, txs).await;
}

#[tokio_test]
async fn initial_state() {
    let _initial_state: InitialStateData<DictStateReader> = create_default_initial_state().await;
}

#[tokio_test]
async fn poc() {
    let mut initial_state: InitialStateData<DictStateReader> = create_default_initial_state().await;
    let txs = poc_txs(&mut initial_state);
    flow_test_body(initial_state, txs.to_vec()).await;
}
