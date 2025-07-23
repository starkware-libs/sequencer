use apollo_batcher::metrics::STORAGE_HEIGHT;
use apollo_consensus::metrics::CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS;
use apollo_infra_utils::run_until::run_until;
use apollo_infra_utils::tracing::{CustomLogger, TraceLevel};
use apollo_monitoring_endpoint::test_utils::MonitoringClient;
use apollo_state_sync_metrics::metrics::{
    STATE_SYNC_BODY_MARKER,
    STATE_SYNC_CLASS_MANAGER_MARKER,
    STATE_SYNC_COMPILED_CLASS_MARKER,
    STATE_SYNC_HEADER_MARKER,
    STATE_SYNC_PROCESSED_TRANSACTIONS,
    STATE_SYNC_STATE_MARKER,
};
use starknet_api::block::BlockNumber;
use tokio::try_join;
use tracing::info;

/// Gets the latest block number from the batcher's metrics.
pub async fn get_batcher_latest_block_number(
    batcher_monitoring_client: &MonitoringClient,
) -> BlockNumber {
    BlockNumber(
        batcher_monitoring_client
            .get_metric::<u64>(STORAGE_HEIGHT.get_name())
            .await
            .expect("Failed to get storage height metric."),
    )
    .prev() // The metric is the height marker so we need to subtract 1 to get the latest.
    .expect("Storage height should be at least 1.")
}

/// Gets the latest decisions reached by consensus from the consensus metrics.
pub async fn get_consensus_decisions_reached(
    consensus_monitoring_client: &MonitoringClient,
) -> u64 {
    consensus_monitoring_client
        .get_metric::<u64>(CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS.get_name())
        .await
        .expect("Failed to get consensus proposals sent metric.")
}

/// Gets the latest block number from the sync's metrics.
async fn get_sync_latest_block_number(sync_monitoring_client: &MonitoringClient) -> BlockNumber {
    let metrics = sync_monitoring_client.get_metrics().await.expect("Failed to get metrics.");

    let latest_marker_value = [
        STATE_SYNC_HEADER_MARKER,
        STATE_SYNC_BODY_MARKER,
        STATE_SYNC_STATE_MARKER,
        STATE_SYNC_CLASS_MANAGER_MARKER,
        STATE_SYNC_COMPILED_CLASS_MARKER,
    ]
    .iter()
    .map(|marker| {
        marker
            .parse_numeric_metric::<u64>(&metrics)
            .unwrap_or_else(|| panic!("Failed to get {} metric.", marker.get_name()))
    })
    // we keep only the positive values because class manager marker is not updated in central sync
    // and compiled class marker is not updated in p2p sync
    .filter(|&marker_value| marker_value > 0)
    // we take the minimum value, or 0 if there are no positive values
    .min()
    .unwrap_or(0);

    BlockNumber(latest_marker_value)
    .prev() // The metric is the height marker so we need to subtract 1 to get the latest.
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

pub async fn await_sync_block(
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

pub async fn await_block(
    batcher_monitoring_client: &MonitoringClient,
    batcher_executable_index: usize,
    state_sync_monitoring_client: &MonitoringClient,
    state_sync_executable_index: usize,
    expected_block_number: BlockNumber,
    node_index: usize,
) {
    info!(
        "Awaiting until {expected_block_number} blocks have been created in sequencer {}.",
        node_index
    );
    let condition =
        |&latest_block_number: &BlockNumber| latest_block_number >= expected_block_number;

    let expected_height = expected_block_number.unchecked_next();
    let [batcher_logger, sync_logger] =
        [("Batcher", batcher_executable_index), ("Sync", state_sync_executable_index)].map(
            |(component_name, executable_index)| {
                CustomLogger::new(
                    TraceLevel::Info,
                    Some(format!(
                        "Waiting for {component_name} height metric to reach block \
                         {expected_height} in sequencer {node_index} executable \
                         {executable_index}.",
                    )),
                )
            },
        );
    // TODO(noamsp): Change this so we get both values with one metrics query.
    try_join!(
        await_batcher_block(5000, condition, 50, batcher_monitoring_client, batcher_logger),
        await_sync_block(5000, condition, 50, state_sync_monitoring_client, sync_logger)
    )
    .unwrap_or_else(|_| {
        panic!(
            "Test conditions of reaching block {expected_block_number} by node {node_index}
            haven't been met."
        )
    });
}

pub async fn verify_txs_accepted(
    monitoring_client: &MonitoringClient,
    sequencer_idx: usize,
    expected_n_accepted_txs: usize,
) {
    info!("Verifying that sequencer {sequencer_idx} accepted {expected_n_accepted_txs} txs.");
    let n_accepted_txs = sequencer_num_accepted_txs(monitoring_client).await;
    assert_eq!(
        n_accepted_txs, expected_n_accepted_txs,
        "Sequencer {sequencer_idx} accepted an unexpected number of txs. Expected \
         {expected_n_accepted_txs} got {n_accepted_txs}"
    );
}

pub async fn await_txs_accepted(
    monitoring_client: &MonitoringClient,
    sequencer_idx: usize,
    target_n_accepted_txs: usize,
) {
    const INTERVAL_MILLIS: u64 = 5000;
    const MAX_ATTEMPTS: usize = 50;
    info!("Waiting until sequencer {sequencer_idx} accepts {target_n_accepted_txs} txs.");

    let condition =
        |&current_n_accepted_txs: &usize| current_n_accepted_txs >= target_n_accepted_txs;

    let get_current_n_accepted_txs_closure = || sequencer_num_accepted_txs(monitoring_client);

    let logger = CustomLogger::new(
        TraceLevel::Info,
        Some(format!(
            "Waiting for sequencer {sequencer_idx} to accept {target_n_accepted_txs} txs.",
        )),
    );

    run_until(
        INTERVAL_MILLIS,
        MAX_ATTEMPTS,
        get_current_n_accepted_txs_closure,
        condition,
        Some(logger),
    )
    .await
    .unwrap_or_else(|| {
        panic!("Sequencer {sequencer_idx} did not accept {target_n_accepted_txs} txs.")
    });
}

pub async fn sequencer_num_accepted_txs(monitoring_client: &MonitoringClient) -> usize {
    // If the sequencer accepted txs, sync should process them and update the respective metric.
    monitoring_client
        .get_metric::<usize>(STATE_SYNC_PROCESSED_TRANSACTIONS.get_name())
        .await
        .unwrap()
}
