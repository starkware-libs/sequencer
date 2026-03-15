use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use reqwest::StatusCode;
use starknet_api::block::{
    BlockNumber,
    BlockTimestamp,
    GasPrice,
    GasPricePerToken,
    StarknetVersion,
};
use starknet_api::core::ContractAddress;
use starknet_api::data_availability::L1DataAvailabilityMode;
use starknet_api::felt;

use crate::cende_client_types::CendeBlockMetadata;
use crate::pre_confirmed_block_writer::{
    CandidateTxReceiver,
    PreconfirmedBlockWriter,
    PreconfirmedBlockWriterInput,
    PreconfirmedBlockWriterTrait,
    PreconfirmedTxReceiver,
};
use crate::pre_confirmed_cende_client::{
    CendeWritePreconfirmedBlock,
    PreconfirmedCendeClientError,
    PreconfirmedCendeClientResult,
    PreconfirmedCendeClientTrait,
};
use crate::test_utils::test_txs;

#[derive(Default)]
struct RecordingClient {
    calls: Mutex<Vec<CendeWritePreconfirmedBlock>>,
    start_times: Mutex<Vec<tokio::time::Instant>>,
    queued_results: Mutex<VecDeque<PreconfirmedCendeClientResult<()>>>,
}

impl RecordingClient {
    fn call_count(&self) -> usize {
        self.calls.lock().expect("calls lock poisoned").len()
    }

    fn start_times(&self) -> Vec<tokio::time::Instant> {
        self.start_times.lock().expect("start_times lock poisoned").clone()
    }

    fn queue_result(&self, result: PreconfirmedCendeClientResult<()>) {
        self.queued_results.lock().expect("queued_results lock poisoned").push_back(result);
    }
}

#[async_trait]
impl PreconfirmedCendeClientTrait for RecordingClient {
    async fn write_pre_confirmed_block(
        &self,
        pre_confirmed_block: CendeWritePreconfirmedBlock,
    ) -> PreconfirmedCendeClientResult<()> {
        self.start_times
            .lock()
            .expect("start_times lock poisoned")
            .push(tokio::time::Instant::now());
        self.calls.lock().expect("calls lock poisoned").push(pre_confirmed_block);

        self.queued_results
            .lock()
            .expect("queued_results lock poisoned")
            .pop_front()
            .unwrap_or(Ok(()))
    }
}

fn test_metadata() -> CendeBlockMetadata {
    let gas_price = GasPrice(1);
    CendeBlockMetadata {
        status: "PENDING",
        starknet_version: StarknetVersion::default(),
        l1_da_mode: L1DataAvailabilityMode::Calldata,
        l1_gas_price: GasPricePerToken { price_in_fri: gas_price, price_in_wei: gas_price },
        l1_data_gas_price: GasPricePerToken { price_in_fri: gas_price, price_in_wei: gas_price },
        l2_gas_price: GasPricePerToken { price_in_fri: gas_price, price_in_wei: gas_price },
        timestamp: BlockTimestamp(0),
        sequencer_address: ContractAddress::try_from(felt!("0x111")).expect("valid test address"),
    }
}

fn build_writer(
    cende_client: Arc<dyn PreconfirmedCendeClientTrait>,
    interval_millis: u64,
    candidate_tx_receiver: CandidateTxReceiver,
    pre_confirmed_tx_receiver: PreconfirmedTxReceiver,
) -> PreconfirmedBlockWriter {
    PreconfirmedBlockWriter::new(
        PreconfirmedBlockWriterInput {
            block_number: BlockNumber(1),
            round: 0,
            block_metadata: test_metadata(),
        },
        candidate_tx_receiver,
        pre_confirmed_tx_receiver,
        cende_client,
        interval_millis,
    )
}

#[tokio::test]
async fn periodic_dirty_updates_are_throttled() {
    let client = Arc::new(RecordingClient::default());
    let (candidate_tx_sender, candidate_tx_receiver) = tokio::sync::mpsc::channel(8);
    let (pre_confirmed_tx_sender, pre_confirmed_tx_receiver) = tokio::sync::mpsc::channel(8);
    let mut writer =
        build_writer(client.clone(), 80, candidate_tx_receiver, pre_confirmed_tx_receiver);

    let handle = tokio::spawn(async move {
        writer.run().await;
    });

    candidate_tx_sender.send(test_txs(0..1)).await.expect("candidate send should succeed");
    tokio::time::sleep(Duration::from_millis(90)).await;
    candidate_tx_sender.send(test_txs(1..2)).await.expect("candidate send should succeed");
    tokio::time::sleep(Duration::from_millis(90)).await;

    drop(candidate_tx_sender);
    drop(pre_confirmed_tx_sender);

    tokio::time::timeout(Duration::from_secs(2), handle)
        .await
        .expect("writer should finish")
        .expect("join should succeed");

    let start_times = client.start_times();
    assert!(start_times.len() >= 2, "expected at least two write starts for two dirty periods");
    for pair in start_times.windows(2) {
        assert!(
            pair[1].duration_since(pair[0]) >= Duration::from_millis(80),
            "write starts should be throttled by write_block_interval_millis"
        );
    }
}

#[tokio::test]
async fn stops_when_either_receiver_closes() {
    let client = Arc::new(RecordingClient::default());
    let (candidate_tx_sender, candidate_tx_receiver) = tokio::sync::mpsc::channel(8);
    let (pre_confirmed_tx_sender, pre_confirmed_tx_receiver) = tokio::sync::mpsc::channel(8);
    let mut writer = build_writer(client, 20, candidate_tx_receiver, pre_confirmed_tx_receiver);

    drop(pre_confirmed_tx_sender);
    drop(candidate_tx_sender);

    tokio::time::timeout(Duration::from_secs(1), async move {
        writer.run().await;
    })
    .await
    .expect("writer should stop when a receiver closes");
}

#[tokio::test]
async fn no_writes_when_no_changes() {
    let client = Arc::new(RecordingClient::default());
    let (candidate_tx_sender, candidate_tx_receiver) = tokio::sync::mpsc::channel(8);
    let (pre_confirmed_tx_sender, pre_confirmed_tx_receiver) = tokio::sync::mpsc::channel(8);
    let mut writer =
        build_writer(client.clone(), 20, candidate_tx_receiver, pre_confirmed_tx_receiver);

    drop(candidate_tx_sender);
    drop(pre_confirmed_tx_sender);

    tokio::time::timeout(Duration::from_secs(1), async move {
        writer.run().await;
    })
    .await
    .expect("writer should stop when channels close");

    assert_eq!(client.call_count(), 0, "writer should not write without dirty updates");
}

#[tokio::test]
async fn final_flush_on_close_when_dirty() {
    let client = Arc::new(RecordingClient::default());
    let (candidate_tx_sender, candidate_tx_receiver) = tokio::sync::mpsc::channel(8);
    let (pre_confirmed_tx_sender, pre_confirmed_tx_receiver) = tokio::sync::mpsc::channel(8);
    let mut writer =
        build_writer(client.clone(), 200, candidate_tx_receiver, pre_confirmed_tx_receiver);

    let handle = tokio::spawn(async move {
        writer.run().await;
    });

    candidate_tx_sender.send(test_txs(0..1)).await.expect("candidate send should succeed");
    tokio::time::sleep(Duration::from_millis(20)).await;
    drop(candidate_tx_sender);
    drop(pre_confirmed_tx_sender);

    tokio::time::timeout(Duration::from_secs(2), handle)
        .await
        .expect("writer should finish")
        .expect("join should succeed");

    assert_eq!(
        client.call_count(),
        1,
        "writer should perform a final flush when closing with dirty state"
    );
}

#[tokio::test]
async fn ignores_write_errors() {
    let client = Arc::new(RecordingClient::default());
    client.queue_result(Err(PreconfirmedCendeClientError::CendeRecorderError {
        block_number: BlockNumber(1),
        round: 0,
        write_iteration: 0,
        status_code: StatusCode::BAD_REQUEST,
    }));
    client.queue_result(Err(PreconfirmedCendeClientError::CendeRecorderError {
        block_number: BlockNumber(1),
        round: 0,
        write_iteration: 1,
        status_code: StatusCode::BAD_REQUEST,
    }));

    let (candidate_tx_sender, candidate_tx_receiver) = tokio::sync::mpsc::channel(8);
    let (pre_confirmed_tx_sender, pre_confirmed_tx_receiver) = tokio::sync::mpsc::channel(8);
    let mut writer =
        build_writer(client.clone(), 20, candidate_tx_receiver, pre_confirmed_tx_receiver);

    let handle = tokio::spawn(async move {
        writer.run().await;
    });

    candidate_tx_sender.send(test_txs(0..1)).await.expect("candidate send should succeed");
    drop(candidate_tx_sender);
    drop(pre_confirmed_tx_sender);

    tokio::time::timeout(Duration::from_secs(2), handle)
        .await
        .expect("writer should finish despite write errors")
        .expect("join should succeed");

    assert!(client.call_count() >= 1, "writer should still attempt writes on errors");
}
