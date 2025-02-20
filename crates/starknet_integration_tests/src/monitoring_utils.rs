use starknet_api::block::BlockNumber;
use starknet_infra_utils::run_until::run_until;
use starknet_infra_utils::tracing::{CustomLogger, TraceLevel};
use starknet_monitoring_endpoint::test_utils::MonitoringClient;
use starknet_sequencer_metrics::metric_definitions;
use tracing::info;

/// Gets the latest block number from the batcher's metrics.
pub async fn get_batcher_latest_block_number(
    batcher_monitoring_client: &MonitoringClient,
) -> BlockNumber {
    BlockNumber(
        batcher_monitoring_client
            .get_metric::<u64>(metric_definitions::STORAGE_HEIGHT.get_name())
            .await
            .expect("Failed to get storage height metric."),
    )
    .prev() // The metric is the height marker so we need to subtract 1 to get the latest.
    .expect("Storage height should be at least 1.")
}

/// Gets the latest block number from the sync's metrics.
pub async fn get_sync_latest_block_number(
    sync_monitoring_client: &MonitoringClient,
) -> BlockNumber {
    let sync_header_marker = sync_monitoring_client
        .get_metric::<u64>(metric_definitions::SYNC_HEADER_MARKER.get_name())
        .await
        .expect("Failed to get sync header marker metric.");
    let sync_body_marker = sync_monitoring_client
        .get_metric::<u64>(metric_definitions::SYNC_BODY_MARKER.get_name())
        .await
        .expect("Failed to get sync body marker metric.");
    let sync_state_marker = sync_monitoring_client
        .get_metric::<u64>(metric_definitions::SYNC_STATE_MARKER.get_name())
        .await
        .expect("Failed to get sync state marker metric.");

    BlockNumber(std::cmp::min(
        sync_header_marker,
        std::cmp::min(sync_body_marker, sync_state_marker),
    ))
    .prev()
    .expect("Sync marker should be at least 1.")
}

/// Sample the metrics until sufficiently many blocks have been reported by the batcher. Returns an
/// error if after the given number of attempts the target block number has not been reached.
pub async fn await_batcher_block(
    interval: u64,
    condition: impl Fn(&BlockNumber) -> bool + Send + Sync,
    max_attempts: usize,
    batcher_monitoring_client: &MonitoringClient,
    logger: CustomLogger,
) -> Result<BlockNumber, ()> {
    let get_latest_block_number_closure =
        || get_batcher_latest_block_number(batcher_monitoring_client);

    run_until(interval, max_attempts, get_latest_block_number_closure, condition, Some(logger))
        .await
        .ok_or(())
}

pub async fn await_sync_block_marker(
    interval: u64,
    condition: impl Fn(&BlockNumber) -> bool + Send + Sync,
    max_attempts: usize,
    sync_monitoring_client: &MonitoringClient,
    logger: CustomLogger,
) -> Result<BlockNumber, ()> {
    let get_latest_block_number_closure = || get_sync_latest_block_number(sync_monitoring_client);

    run_until(interval, max_attempts, get_latest_block_number_closure, condition, Some(logger))
        .await
        .ok_or(())
}

pub async fn await_execution(
    monitoring_client: &MonitoringClient,
    expected_block_number: BlockNumber,
    node_index: usize,
    batcher_index: usize,
) {
    info!(
        "Awaiting until {expected_block_number} blocks have been created in sequencer {}.",
        node_index
    );
    let condition =
        |&latest_block_number: &BlockNumber| latest_block_number >= expected_block_number;

    let logger = CustomLogger::new(
        TraceLevel::Info,
        Some(format!(
            "Waiting for batcher height metric to reach block {expected_block_number} in \
             sequencer {} executable {}.",
            node_index, batcher_index
        )),
    );
    await_batcher_block(5000, condition, 50, monitoring_client, logger)
        .await
        .expect("Block number should have been reached.");
}

pub async fn verify_txs_accepted(
    monitoring_client: &MonitoringClient,
    sequencer_idx: usize,
    expected_n_batched_txs: usize,
) {
    info!("Verifying that sequencer {sequencer_idx} got {expected_n_batched_txs} batched txs.");
    let n_batched_txs = monitoring_client
        .get_metric::<usize>(metric_definitions::BATCHED_TRANSACTIONS.get_name())
        .await
        .expect("Failed to get batched txs metric.");
    assert_eq!(
        n_batched_txs, expected_n_batched_txs,
        "Sequencer {sequencer_idx} got an unexpected number of batched txs. Expected \
         {expected_n_batched_txs} got {n_batched_txs}"
    );
}
