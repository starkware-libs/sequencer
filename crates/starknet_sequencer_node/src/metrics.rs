use std::sync::Arc;

use starknet_sequencer_infra::metrics::{create_shared_infra_metrics, InfraMetricsTrait};
use starknet_sequencer_metrics::metric_definitions::{
    BATCHER_MSGS_PROCESSED,
    BATCHER_MSGS_RECEIVED,
    BATCHER_QUEUE_DEPTH,
    CLASS_MANAGER_MSGS_PROCESSED,
    CLASS_MANAGER_MSGS_RECEIVED,
    CLASS_MANAGER_QUEUE_DEPTH,
    GATEWAY_MSGS_PROCESSED,
    GATEWAY_MSGS_RECEIVED,
    GATEWAY_QUEUE_DEPTH,
    L1_PROVIDER_MSGS_PROCESSED,
    L1_PROVIDER_MSGS_RECEIVED,
    L1_PROVIDER_QUEUE_DEPTH,
    MEMPOOL_MSGS_PROCESSED,
    MEMPOOL_MSGS_RECEIVED,
    MEMPOOL_P2P_MSGS_PROCESSED,
    MEMPOOL_P2P_MSGS_RECEIVED,
    MEMPOOL_P2P_QUEUE_DEPTH,
    MEMPOOL_QUEUE_DEPTH,
    SIERRA_COMPILER_MSGS_PROCESSED,
    SIERRA_COMPILER_MSGS_RECEIVED,
    SIERRA_COMPILER_QUEUE_DEPTH,
    STATE_SYNC_MSGS_PROCESSED,
    STATE_SYNC_MSGS_RECEIVED,
    STATE_SYNC_QUEUE_DEPTH,
};

pub struct LocalServersMetric {
    pub batcher_metrics: Arc<dyn InfraMetricsTrait>,
    pub class_manager_metrics: Arc<dyn InfraMetricsTrait>,
    pub gateway_metrics: Arc<dyn InfraMetricsTrait>,
    pub l1_provider_metrics: Arc<dyn InfraMetricsTrait>,
    pub mempool_metrics: Arc<dyn InfraMetricsTrait>,
    pub mempool_p2p_metrics: Arc<dyn InfraMetricsTrait>,
    pub sierra_compiler_metrics: Arc<dyn InfraMetricsTrait>,
    pub state_sync_metrics: Arc<dyn InfraMetricsTrait>,
}

pub fn create_local_servers_metrics() -> LocalServersMetric {
    LocalServersMetric {
        batcher_metrics: create_shared_infra_metrics(
            &BATCHER_MSGS_RECEIVED,
            &BATCHER_MSGS_PROCESSED,
            &BATCHER_QUEUE_DEPTH,
        ),
        class_manager_metrics: create_shared_infra_metrics(
            &CLASS_MANAGER_MSGS_RECEIVED,
            &CLASS_MANAGER_MSGS_PROCESSED,
            &CLASS_MANAGER_QUEUE_DEPTH,
        ),
        gateway_metrics: create_shared_infra_metrics(
            &GATEWAY_MSGS_RECEIVED,
            &GATEWAY_MSGS_PROCESSED,
            &GATEWAY_QUEUE_DEPTH,
        ),
        l1_provider_metrics: create_shared_infra_metrics(
            &L1_PROVIDER_MSGS_RECEIVED,
            &L1_PROVIDER_MSGS_PROCESSED,
            &L1_PROVIDER_QUEUE_DEPTH,
        ),
        mempool_metrics: create_shared_infra_metrics(
            &MEMPOOL_MSGS_RECEIVED,
            &MEMPOOL_MSGS_PROCESSED,
            &MEMPOOL_QUEUE_DEPTH,
        ),
        mempool_p2p_metrics: create_shared_infra_metrics(
            &MEMPOOL_P2P_MSGS_RECEIVED,
            &MEMPOOL_P2P_MSGS_PROCESSED,
            &MEMPOOL_P2P_QUEUE_DEPTH,
        ),
        sierra_compiler_metrics: create_shared_infra_metrics(
            &SIERRA_COMPILER_MSGS_RECEIVED,
            &SIERRA_COMPILER_MSGS_PROCESSED,
            &SIERRA_COMPILER_QUEUE_DEPTH,
        ),
        state_sync_metrics: create_shared_infra_metrics(
            &STATE_SYNC_MSGS_RECEIVED,
            &STATE_SYNC_MSGS_PROCESSED,
            &STATE_SYNC_QUEUE_DEPTH,
        ),
    }
}
