use crate::dashboard::{Dashboard, Row};
use crate::panels::batcher::{get_batcher_infra_row, get_batcher_row};
use crate::panels::class_manager::get_class_manager_infra_row;
use crate::panels::consensus::{
    PANEL_CENDE_LAST_PREPARED_BLOB_BLOCK_NUMBER,
    PANEL_CENDE_PREPARE_BLOB_FOR_NEXT_HEIGHT_LATENCY,
    PANEL_CENDE_WRITE_BLOB_FAILURE,
    PANEL_CENDE_WRITE_BLOB_SUCCESS,
    PANEL_CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY,
    PANEL_CONSENSUS_BLOCK_NUMBER,
    PANEL_CONSENSUS_BUILD_PROPOSAL_FAILED,
    PANEL_CONSENSUS_BUILD_PROPOSAL_TOTAL,
    PANEL_CONSENSUS_CACHED_VOTES,
    PANEL_CONSENSUS_CONFLICTING_VOTES,
    PANEL_CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS,
    PANEL_CONSENSUS_DECISIONS_REACHED_BY_SYNC,
    PANEL_CONSENSUS_HELD_LOCKS,
    PANEL_CONSENSUS_INBOUND_STREAM_EVICTED,
    PANEL_CONSENSUS_INBOUND_STREAM_FINISHED,
    PANEL_CONSENSUS_INBOUND_STREAM_STARTED,
    PANEL_CONSENSUS_L1_DATA_GAS_MISMATCH,
    PANEL_CONSENSUS_L1_GAS_MISMATCH,
    PANEL_CONSENSUS_L2_GAS_PRICE,
    PANEL_CONSENSUS_MAX_CACHED_BLOCK_NUMBER,
    PANEL_CONSENSUS_NEW_VALUE_LOCKS,
    PANEL_CONSENSUS_NUM_BATCHES_IN_PROPOSAL,
    PANEL_CONSENSUS_NUM_CONNECTED_PEERS,
    PANEL_CONSENSUS_NUM_TXS_IN_PROPOSAL,
    PANEL_CONSENSUS_OUTBOUND_STREAM_FINISHED,
    PANEL_CONSENSUS_OUTBOUND_STREAM_STARTED,
    PANEL_CONSENSUS_PROPOSALS_INVALID,
    PANEL_CONSENSUS_PROPOSALS_NUM_RECEIVED_MESSAGES,
    PANEL_CONSENSUS_PROPOSALS_NUM_SENT_MESSAGES,
    PANEL_CONSENSUS_PROPOSALS_RECEIVED,
    PANEL_CONSENSUS_PROPOSALS_VALIDATED,
    PANEL_CONSENSUS_PROPOSALS_VALID_INIT,
    PANEL_CONSENSUS_REPROPOSALS,
    PANEL_CONSENSUS_ROUND,
    PANEL_CONSENSUS_ROUND_ABOVE_ZERO,
    PANEL_CONSENSUS_ROUND_AVG,
    PANEL_CONSENSUS_TIMEOUTS_BY_TYPE,
    PANEL_CONSENSUS_VOTES_NUM_RECEIVED_MESSAGES,
    PANEL_CONSENSUS_VOTES_NUM_SENT_MESSAGES,
};
use crate::panels::gateway::{get_gateway_infra_row, get_gateway_row};
use crate::panels::http_server::PANEL_ADDED_TRANSACTIONS_TOTAL;
use crate::panels::l1_gas_price::{
    PANEL_ETH_TO_STRK_ERROR_COUNT,
    PANEL_L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY,
    PANEL_L1_GAS_PRICE_PROVIDER_LOCAL_MSGS_PROCESSED,
    PANEL_L1_GAS_PRICE_PROVIDER_LOCAL_MSGS_RECEIVED,
    PANEL_L1_GAS_PRICE_PROVIDER_LOCAL_QUEUE_DEPTH,
    PANEL_L1_GAS_PRICE_PROVIDER_REMOTE_CLIENT_SEND_ATTEMPTS,
    PANEL_L1_GAS_PRICE_PROVIDER_REMOTE_MSGS_PROCESSED,
    PANEL_L1_GAS_PRICE_PROVIDER_REMOTE_MSGS_RECEIVED,
    PANEL_L1_GAS_PRICE_PROVIDER_REMOTE_VALID_MSGS_RECEIVED,
    PANEL_L1_GAS_PRICE_SCRAPER_BASELAYER_ERROR_COUNT,
    PANEL_L1_GAS_PRICE_SCRAPER_REORG_DETECTED,
};
use crate::panels::l1_provider::{
    PANEL_L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT,
    PANEL_L1_MESSAGE_SCRAPER_REORG_DETECTED,
    PANEL_L1_PROVIDER_LOCAL_MSGS_PROCESSED,
    PANEL_L1_PROVIDER_LOCAL_MSGS_RECEIVED,
    PANEL_L1_PROVIDER_LOCAL_QUEUE_DEPTH,
    PANEL_L1_PROVIDER_REMOTE_CLIENT_SEND_ATTEMPTS,
    PANEL_L1_PROVIDER_REMOTE_MSGS_PROCESSED,
    PANEL_L1_PROVIDER_REMOTE_MSGS_RECEIVED,
    PANEL_L1_PROVIDER_REMOTE_VALID_MSGS_RECEIVED,
};
use crate::panels::mempool::{get_mempool_infra_row, get_mempool_row};
use crate::panels::mempool_p2p::{get_mempool_p2p_infra_row, get_mempool_p2p_row};
use crate::panels::sierra_compiler::{
    PANEL_COMPILATION_DURATION,
    PANEL_SIERRA_COMPILER_LOCAL_MSGS_PROCESSED,
    PANEL_SIERRA_COMPILER_LOCAL_MSGS_RECEIVED,
    PANEL_SIERRA_COMPILER_LOCAL_QUEUE_DEPTH,
    PANEL_SIERRA_COMPILER_REMOTE_CLIENT_SEND_ATTEMPTS,
    PANEL_SIERRA_COMPILER_REMOTE_MSGS_PROCESSED,
    PANEL_SIERRA_COMPILER_REMOTE_MSGS_RECEIVED,
    PANEL_SIERRA_COMPILER_REMOTE_VALID_MSGS_RECEIVED,
};
use crate::panels::state_reader::{
    PANEL_BLOCKIFIER_STATE_READER_CLASS_CACHE_MISS_RATIO,
    PANEL_BLOCKIFIER_STATE_READER_NATIVE_CLASS_RETURNED_RATIO,
    PANEL_NATIVE_COMPILATION_ERROR,
};
use crate::panels::state_sync::{
    get_state_sync_infra_row,
    get_state_sync_p2p_row,
    get_state_sync_row,
};

#[cfg(test)]
#[path = "dashboard_definitions_test.rs"]
mod dashboard_definitions_test;

// TODO(Tsabary): this file should be managed by this crate, hence should be moved here to a
// resources folder.
pub const DEV_JSON_PATH: &str = "Monitoring/sequencer/dev_grafana.json";

fn get_sierra_compiler_infra_row() -> Row {
    Row::new(
        "SierraCompilerInfra",
        vec![
            PANEL_SIERRA_COMPILER_LOCAL_MSGS_RECEIVED,
            PANEL_SIERRA_COMPILER_LOCAL_MSGS_PROCESSED,
            PANEL_SIERRA_COMPILER_LOCAL_QUEUE_DEPTH,
            PANEL_SIERRA_COMPILER_REMOTE_MSGS_RECEIVED,
            PANEL_SIERRA_COMPILER_REMOTE_VALID_MSGS_RECEIVED,
            PANEL_SIERRA_COMPILER_REMOTE_MSGS_PROCESSED,
            PANEL_SIERRA_COMPILER_REMOTE_CLIENT_SEND_ATTEMPTS,
        ],
    )
}

fn get_consensus_p2p_row() -> Row {
    Row::new(
        "ConsensusP2p",
        vec![
            PANEL_CONSENSUS_NUM_CONNECTED_PEERS,
            PANEL_CONSENSUS_VOTES_NUM_SENT_MESSAGES,
            PANEL_CONSENSUS_VOTES_NUM_RECEIVED_MESSAGES,
            PANEL_CONSENSUS_PROPOSALS_NUM_SENT_MESSAGES,
            PANEL_CONSENSUS_PROPOSALS_NUM_RECEIVED_MESSAGES,
            PANEL_CONSENSUS_CONFLICTING_VOTES,
        ],
    )
}

fn get_l1_provider_infra_row() -> Row {
    Row::new(
        "L1 Provider Infra",
        vec![
            PANEL_L1_PROVIDER_LOCAL_MSGS_RECEIVED,
            PANEL_L1_PROVIDER_LOCAL_MSGS_PROCESSED,
            PANEL_L1_PROVIDER_LOCAL_QUEUE_DEPTH,
            PANEL_L1_PROVIDER_REMOTE_MSGS_RECEIVED,
            PANEL_L1_PROVIDER_REMOTE_VALID_MSGS_RECEIVED,
            PANEL_L1_PROVIDER_REMOTE_MSGS_PROCESSED,
            PANEL_L1_PROVIDER_REMOTE_CLIENT_SEND_ATTEMPTS,
        ],
    )
}

fn get_l1_gas_price_infra_row() -> Row {
    Row::new(
        "L1 Gas Price Infra",
        vec![
            PANEL_L1_GAS_PRICE_PROVIDER_LOCAL_MSGS_RECEIVED,
            PANEL_L1_GAS_PRICE_PROVIDER_LOCAL_MSGS_PROCESSED,
            PANEL_L1_GAS_PRICE_PROVIDER_LOCAL_QUEUE_DEPTH,
            PANEL_L1_GAS_PRICE_PROVIDER_REMOTE_MSGS_RECEIVED,
            PANEL_L1_GAS_PRICE_PROVIDER_REMOTE_VALID_MSGS_RECEIVED,
            PANEL_L1_GAS_PRICE_PROVIDER_REMOTE_MSGS_PROCESSED,
            PANEL_L1_GAS_PRICE_PROVIDER_REMOTE_CLIENT_SEND_ATTEMPTS,
        ],
    )
}

fn get_blockifier_state_reader_row() -> Row {
    Row::new(
        "Blockifier State Reader",
        vec![
            PANEL_BLOCKIFIER_STATE_READER_CLASS_CACHE_MISS_RATIO,
            PANEL_BLOCKIFIER_STATE_READER_NATIVE_CLASS_RETURNED_RATIO,
            PANEL_NATIVE_COMPILATION_ERROR,
        ],
    )
}

fn get_consensus_row() -> Row {
    Row::new(
        "Consensus",
        vec![
            PANEL_CONSENSUS_BLOCK_NUMBER,
            PANEL_CONSENSUS_ROUND,
            PANEL_CONSENSUS_ROUND_AVG,
            PANEL_CONSENSUS_ROUND_ABOVE_ZERO,
            PANEL_CONSENSUS_MAX_CACHED_BLOCK_NUMBER,
            PANEL_CONSENSUS_CACHED_VOTES,
            PANEL_CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS,
            PANEL_CONSENSUS_DECISIONS_REACHED_BY_SYNC,
            PANEL_CONSENSUS_PROPOSALS_RECEIVED,
            PANEL_CONSENSUS_PROPOSALS_VALID_INIT,
            PANEL_CONSENSUS_PROPOSALS_VALIDATED,
            PANEL_CONSENSUS_PROPOSALS_INVALID,
            PANEL_CONSENSUS_BUILD_PROPOSAL_TOTAL,
            PANEL_CONSENSUS_BUILD_PROPOSAL_FAILED,
            PANEL_CONSENSUS_REPROPOSALS,
            PANEL_CONSENSUS_NEW_VALUE_LOCKS,
            PANEL_CONSENSUS_HELD_LOCKS,
            PANEL_CONSENSUS_TIMEOUTS_BY_TYPE,
            PANEL_CONSENSUS_NUM_BATCHES_IN_PROPOSAL,
            PANEL_CONSENSUS_NUM_TXS_IN_PROPOSAL,
            PANEL_CONSENSUS_INBOUND_STREAM_STARTED,
            PANEL_CONSENSUS_INBOUND_STREAM_EVICTED,
            PANEL_CONSENSUS_INBOUND_STREAM_FINISHED,
            PANEL_CONSENSUS_OUTBOUND_STREAM_STARTED,
            PANEL_CONSENSUS_OUTBOUND_STREAM_FINISHED,
            PANEL_CONSENSUS_L2_GAS_PRICE,
            PANEL_CENDE_LAST_PREPARED_BLOB_BLOCK_NUMBER,
            PANEL_CENDE_PREPARE_BLOB_FOR_NEXT_HEIGHT_LATENCY,
            PANEL_CENDE_WRITE_PREV_HEIGHT_BLOB_LATENCY,
            PANEL_CENDE_WRITE_BLOB_SUCCESS,
            PANEL_CENDE_WRITE_BLOB_FAILURE,
            PANEL_CONSENSUS_L1_DATA_GAS_MISMATCH,
            PANEL_CONSENSUS_L1_GAS_MISMATCH,
        ],
    )
}

fn get_http_server_row() -> Row {
    Row::new("Http Server", vec![PANEL_ADDED_TRANSACTIONS_TOTAL])
}

// TODO(MatanM/GuyN): add l1 gas price row to the dashboard when relevant, and delete the
// annotation.
#[allow(dead_code)]
fn get_l1_gas_price_row() -> Row {
    Row::new(
        "L1 Gas Price",
        vec![
            PANEL_ETH_TO_STRK_ERROR_COUNT,
            PANEL_L1_GAS_PRICE_PROVIDER_INSUFFICIENT_HISTORY,
            PANEL_L1_GAS_PRICE_SCRAPER_BASELAYER_ERROR_COUNT,
            PANEL_L1_GAS_PRICE_SCRAPER_REORG_DETECTED,
        ],
    )
}

// TODO(MatanM/GuyN): add l1 gas price row to the dashboard when relevant, and delete the
// annotation.
#[allow(dead_code)]
fn get_l1_provider_row() -> Row {
    Row::new(
        "L1 Provider",
        vec![
            PANEL_L1_MESSAGE_SCRAPER_BASELAYER_ERROR_COUNT,
            PANEL_L1_MESSAGE_SCRAPER_REORG_DETECTED,
        ],
    )
}

fn get_compile_to_casm_row() -> Row {
    Row::new("Compile sierra to casm", vec![PANEL_COMPILATION_DURATION])
}

pub fn get_apollo_dashboard() -> Dashboard {
    Dashboard::new(
        "Sequencer Node Dashboard",
        vec![
            get_batcher_row(),
            get_consensus_row(),
            get_http_server_row(),
            get_state_sync_row(),
            get_mempool_p2p_row(),
            get_consensus_p2p_row(),
            get_state_sync_p2p_row(),
            get_gateway_row(),
            get_mempool_row(),
            get_blockifier_state_reader_row(),
            get_batcher_infra_row(),
            get_gateway_infra_row(),
            get_class_manager_infra_row(),
            get_l1_provider_infra_row(),
            get_l1_gas_price_infra_row(),
            get_mempool_infra_row(),
            get_mempool_p2p_infra_row(),
            get_sierra_compiler_infra_row(),
            get_compile_to_casm_row(),
            get_state_sync_infra_row(),
        ],
    )
}
