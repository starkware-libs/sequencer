use apollo_batcher::metrics::{BUILDING_HEIGHT, REVERTED_TRANSACTIONS};
use apollo_consensus::metrics::CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS;
use apollo_infra_utils::run_until::run_until;
use apollo_infra_utils::tracing::{CustomLogger, TraceLevel};
use apollo_metrics::metrics::MetricDetails;
use apollo_monitoring_endpoint::test_utils::{MonitoringClient, MonitoringClientError};
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

// TODO(NoamSp): consider changing these consts to input values determined by the specific tested
// scenario.
const INTERVAL_MS: u64 = 100;
const ATTEMPTS_FOR_BLOCK: usize = 2500;
const ATTEMPTS_FOR_VERIFY_TXS: usize = 1000;
const ATTEMPTS_FOR_AWAIT_TXS: usize = 2500;

/// Gets the latest block number from the batcher's metrics.
/// If the metric is not yet registered, or building height is zero, returns None.
pub async fn get_batcher_latest_block_number(
    batcher_monitoring_client: &MonitoringClient,
) -> Option<BlockNumber> {
    let height = match batcher_monitoring_client.get_metric::<u64>(BUILDING_HEIGHT.get_name()).await
    {
        Ok(h) => h,
        Err(MonitoringClientError::MetricNotFound { .. }) => return None,
        Err(e) => panic!("Failed to get storage height metric: {e}"),
    };
    BlockNumber(height).prev()
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
/// If the metric is not yet registered or the latest marker is zero, returns None.
async fn get_sync_latest_block_number(
    sync_monitoring_client: &MonitoringClient,
) -> Option<BlockNumber> {
    let metrics = match sync_monitoring_client.get_metrics().await {
        Ok(metrics) => metrics,
        Err(MonitoringClientError::MetricNotFound { .. }) => return None,
        Err(e) => panic!("Failed to get metrics: {e}"),
    };

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

    // The metric is the height marker so we need to subtract 1 to get the latest.
    BlockNumber(latest_marker_value).prev()
}

/// Sample the metrics until sufficiently many blocks have been reported by the batcher. Returns an
/// error if after the given number of attempts the target block number has not been reached.
pub async fn await_batcher_block(
    interval: u64,
    condition: impl Fn(&Option<BlockNumber>) -> bool + Send + Sync,
    max_attempts: usize,
    batcher_monitoring_client: &MonitoringClient,
    logger: CustomLogger,
) -> Result<BlockNumber, ()> {
    let get_latest_block_number_closure =
        || get_batcher_latest_block_number(batcher_monitoring_client);

    match run_until(
        interval,
        max_attempts,
        get_latest_block_number_closure,
        condition,
        Some(logger),
    )
    .await
    {
        Some(Some(block_number)) => Ok(block_number),
        Some(None) => panic!("The height being built should be at least 1."),
        None => Err(()),
    }
}

pub async fn await_sync_block(
    interval: u64,
    condition: impl Fn(&Option<BlockNumber>) -> bool + Send + Sync,
    max_attempts: usize,
    sync_monitoring_client: &MonitoringClient,
    logger: CustomLogger,
) -> Result<BlockNumber, ()> {
    let get_latest_block_number_closure = || get_sync_latest_block_number(sync_monitoring_client);

    match run_until(
        interval,
        max_attempts,
        get_latest_block_number_closure,
        condition,
        Some(logger),
    )
    .await
    {
        Some(Some(block_number)) => Ok(block_number),
        Some(None) => panic!("Sync marker should be at least 1."),
        None => Err(()),
    }
}

pub async fn await_block(
    batcher_monitoring_client: &MonitoringClient,
    state_sync_monitoring_client: &MonitoringClient,
    expected_block_number: BlockNumber,
    node_index: usize,
) {
    info!(
        "Awaiting until {expected_block_number} blocks have been created in sequencer {}.",
        node_index
    );
    let condition = |latest_block_number: &Option<BlockNumber>| {
        latest_block_number.is_some_and(|block_number| block_number >= expected_block_number)
    };

    let expected_height = expected_block_number.unchecked_next();
    let [batcher_logger, sync_logger] = ["Batcher", "Sync"].map(|component_name| {
        CustomLogger::new(
            TraceLevel::Info,
            Some(format!(
                "Waiting for {component_name} height metric to reach block {expected_height} in \
                 sequencer {node_index}.",
            )),
        )
    });
    // TODO(noamsp): Change this so we get both values with one metrics query.
    try_join!(
        await_batcher_block(
            INTERVAL_MS,
            condition,
            ATTEMPTS_FOR_BLOCK,
            batcher_monitoring_client,
            batcher_logger
        ),
        await_sync_block(
            INTERVAL_MS,
            condition,
            ATTEMPTS_FOR_BLOCK,
            state_sync_monitoring_client,
            sync_logger
        )
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
    let condition = |num_accpted_tx: &usize| *num_accpted_tx >= expected_n_accepted_txs;

    let n_accepted_txs_closure = || sequencer_num_accepted_txs(monitoring_client);

    run_until(INTERVAL_MS, ATTEMPTS_FOR_VERIFY_TXS, n_accepted_txs_closure, condition, None).await;
}

// TODO(Tsabary/NoamSp): check for code duplications w.r.t. all variants of `verify_txs_accepted`
// and `await_txs_accepted`.
pub async fn await_txs_accepted(
    monitoring_client: &MonitoringClient,
    sequencer_idx: usize,
    target_n_accepted_txs: usize,
) {
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
        INTERVAL_MS,
        ATTEMPTS_FOR_AWAIT_TXS,
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
    // Return 0 if the metric isn't registered yet (race with StateSyncRunner startup).
    match monitoring_client.get_metric::<usize>(STATE_SYNC_PROCESSED_TRANSACTIONS.get_name()).await
    {
        Ok(count) => count,
        Err(MonitoringClientError::MetricNotFound { .. }) => 0,
        Err(e) => panic!("Failed to get processed transactions metric: {e}"),
    }
}

pub async fn assert_no_reverted_txs(monitoring_client: &MonitoringClient, sequencer_idx: usize) {
    let get_metric_closure =
        || async { monitoring_client.get_metric::<usize>(REVERTED_TRANSACTIONS.get_name()).await };
    let condition = |result: &Result<usize, MonitoringClientError>| result.is_ok();
    let logger = CustomLogger::new(
        TraceLevel::Info,
        Some(format!(
            "Waiting for reverted transactions metric to be registered for sequencer \
             {sequencer_idx}"
        )),
    );

    let reverted_count = run_until(
        INTERVAL_MS,
        ATTEMPTS_FOR_VERIFY_TXS,
        get_metric_closure,
        condition,
        Some(logger),
    )
    .await
    .expect("Failed to get reverted transactions metric after retries")
    .expect("Metric retrieval failed");

    assert_eq!(
        reverted_count, 0,
        "Sequencer {sequencer_idx} has {reverted_count} reverted transactions"
    );
}
