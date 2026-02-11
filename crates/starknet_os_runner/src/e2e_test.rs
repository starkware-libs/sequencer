use apollo_transaction_converter::proof_verification::verify_proof;
use apollo_transaction_converter::transaction_converter::BOOTLOADER_PROGRAM_HASH;
use blockifier_reexecution::state_reader::rpc_objects::BlockId;

use crate::config::ProverConfig;
use crate::runner::RunnerConfig;
use crate::test_utils::{fetch_real_v3_invoke, get_latest_block_number, get_rpc_url};
use crate::virtual_snos_prover::VirtualSnosProver;

/// End-to-end test: prove a real transaction and verify the proof.
///
/// This test exercises the full proving pipeline:
/// 1. Fetches a real V3 invoke transaction from a recent mainnet block.
/// 2. Proves it using VirtualSnosProver (OS execution + stwo proving),
///    executing against the state from the block before the transaction.
/// 3. Verifies the proof and proof facts using verify_proof (stwo verification,
///    proof facts consistency, proof version, and bootloader program hash).
///
/// Using a real on-chain transaction ensures that resource bounds, nonces, and
/// state are all consistent, avoiding issues with constructed transactions.
///
/// # Requirements
///
/// - `NODE_URL` env var pointing to a Starknet mainnet RPC node.
/// - `stwo_run_and_prove` binary available in PATH.
///
/// # Running
///
/// ```bash
/// NODE_URL=https://your-rpc-node cargo test -p starknet_os_runner -- --ignored test_e2e
/// ```
#[tokio::test]
#[ignore]
async fn test_e2e_prove_and_verify_transaction() {
    // Fetch a real V3 invoke with generous gas bounds from a recent block.
    // Tries several recent blocks to find a suitable transaction.
    let (rpc_tx, tx_block_number) = tokio::task::spawn_blocking(|| {
        let latest = get_latest_block_number();
        for offset in 5..20 {
            let block = latest - offset;
            if let Ok(result) = std::panic::catch_unwind(|| fetch_real_v3_invoke(block)) {
                return result;
            }
        }
        panic!("No suitable V3 invoke found in recent blocks");
    })
    .await
    .unwrap();

    // Execute at the block before the transaction's block so nonce/state match.
    let execution_block_id = BlockId::Number(starknet_api::block::BlockNumber(tx_block_number - 1));

    let config = ProverConfig {
        rpc_node_url: get_rpc_url(),
        runner_config: RunnerConfig::default(),
        ..ProverConfig::default()
    };
    let prover = VirtualSnosProver::new(&config);

    // Prove the transaction.
    let output = prover
        .prove_transaction(execution_block_id, rpc_tx)
        .await
        .expect("prove_transaction should succeed");
    let result = output.result;

    // Verify the proof and proof facts using the same verification logic as the gateway.
    verify_proof(result.proof_facts, result.proof, BOOTLOADER_PROGRAM_HASH)
        .expect("verify_proof should succeed");
}
