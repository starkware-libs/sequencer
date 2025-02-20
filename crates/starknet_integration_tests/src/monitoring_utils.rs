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

pub async fn await_block(
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
    let n_batched_txs = get_batched_transactions_metric(monitoring_client).await;
    assert_eq!(
        n_batched_txs, expected_n_batched_txs,
        "Sequencer {sequencer_idx} got an unexpected number of batched txs. Expected \
         {expected_n_batched_txs} got {n_batched_txs}"
    );
}

// TODO(noamsp): If verify_txs_accepted is changed to use sync metrics, change await_txs_accepted
// as well.
pub async fn await_txs_accepted(
    monitoring_client: &MonitoringClient,
    sequencer_idx: usize,
    target_n_batched_txs: usize,
) {
    const INTERVAL_MILLIS: u64 = 5000;
    const MAX_ATTEMPTS: usize = 50;
    info!("Waiting until sequencer {sequencer_idx} gets {target_n_batched_txs} batched txs.");

    let condition =
        |&current_num_batched_txs: &usize| current_num_batched_txs >= target_n_batched_txs;

    let get_current_num_batched_txs_closure = || get_batched_transactions_metric(monitoring_client);

    let logger = CustomLogger::new(
        TraceLevel::Info,
        Some(format!(
            "Waiting for batcher to batch {target_n_batched_txs} in sequencer {sequencer_idx}.",
        )),
    );

    run_until(
        INTERVAL_MILLIS,
        MAX_ATTEMPTS,
        get_current_num_batched_txs_closure,
        condition,
        Some(logger),
    )
    .await
    .unwrap_or_else(|| {
        panic!("Sequencer {sequencer_idx} did not batch {target_n_batched_txs} transactions.")
    });
}

async fn get_batched_transactions_metric(monitoring_client: &MonitoringClient) -> usize {
    monitoring_client
        .get_metric::<usize>(metric_definitions::BATCHED_TRANSACTIONS.get_name())
        .await
        .expect("Failed to get batched txs metric.")
}
