use std::sync::Arc;

use assert_matches::assert_matches;
use blockifier::blockifier::config::TransactionExecutorConfig;
use blockifier::bouncer::BouncerConfig;
use blockifier::context::ChainInfo;
use blockifier::state::global_cache::GlobalContractCache;
use blockifier::versioned_constants::VersionedConstantsOverrides;
use rstest::{fixture, rstest};
use starknet_api::block::BlockNumber;
use starknet_mempool_types::communication::MockMempoolClient;

use crate::block_builder::{ExecutionConfig, ExecutionParams};
use crate::proposals_manager::{ProposalsManager, ProposalsManagerConfig, ProposalsManagerError};

const GENERATION_TIMEOUT: tokio::time::Duration = tokio::time::Duration::from_secs(1);

#[fixture]
fn execution_params() -> ExecutionParams {
    let execution_config = ExecutionConfig {
        chain_info: ChainInfo::create_for_testing(),
        execute_config: TransactionExecutorConfig::create_for_testing(),
        bouncer_config: BouncerConfig::max(),
        sequencer_address: Default::default(),
        use_kzg_da: Default::default(),
        txs_chunk_size: 1,
        versioned_constants_overrides: VersionedConstantsOverrides {
            validate_max_n_steps: 100000,
            max_recursion_depth: 50,
            invoke_tx_max_n_steps: 100000,
        },
    };
    ExecutionParams { execution_config, global_class_hash_to_class: GlobalContractCache::new(1) }
}

#[rstest]
#[tokio::test]
async fn multiple_proposals_generation_fails(execution_params: ExecutionParams) {
    let mut mempool_client = MockMempoolClient::new();
    mempool_client.expect_get_txs().returning(|_| Ok(vec![]));
    let mut proposals_manager = ProposalsManager::new(
        ProposalsManagerConfig::default(),
        execution_params,
        Arc::new(mempool_client),
    );
    let _ = proposals_manager
        .generate_block_proposal(
            0,
            tokio::time::Instant::now() + GENERATION_TIMEOUT,
            BlockNumber::default(),
        )
        .await
        .unwrap();

    let another_generate_request = proposals_manager
        .generate_block_proposal(
            1,
            tokio::time::Instant::now() + GENERATION_TIMEOUT,
            BlockNumber::default(),
        )
        .await;

    assert_matches!(
        another_generate_request,
        Err(ProposalsManagerError::AlreadyGeneratingProposal {
            current_generating_proposal_id,
            new_proposal_id
        }) if current_generating_proposal_id == 0 && new_proposal_id == 1
    );
}
