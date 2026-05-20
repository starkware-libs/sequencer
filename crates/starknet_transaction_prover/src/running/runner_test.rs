use blockifier::state::state_api::StateReader;
use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use blockifier_reexecution::state_reader::rpc_state_reader::RpcStateReader;
use blockifier_reexecution::utils::get_chain_info;
use blockifier_test_utils::calldata::create_calldata;
use starknet_api::abi::abi_utils::get_storage_var_address;
use starknet_api::core::{ChainId, ContractAddress};
use starknet_api::test_utils::invoke::invoke_tx;
use starknet_api::transaction::fields::ValidResourceBounds;
use starknet_api::transaction::InvokeTransaction;
use starknet_api::{contract_address, invoke_tx_args};

use crate::running::runner::VirtualSnosRunner;
use crate::test_utils::{
    resolve_test_mode,
    resource_bounds_for_client_side_tx,
    runner_factory,
    DUMMY_ACCOUNT_ADDRESS,
    STRK_TOKEN_ADDRESS_SEPOLIA,
};

/// Reads DUMMY's STRK balance via RPC and constructs a `transfer` that drains it entirely,
/// producing a `state_diff` entry where DUMMY's `ERC20_balances` slot transitions to zero.
/// Fees are not charged (resource bounds have `max_price=0`), so this is purely local
/// simulation — no on-chain side effect.
fn construct_drain_balance_invoke(rpc_url: String, block_id: BlockId) -> InvokeTransaction {
    let strk_token = ContractAddress::try_from(STRK_TOKEN_ADDRESS_SEPOLIA).unwrap();
    let dummy = ContractAddress::try_from(DUMMY_ACCOUNT_ADDRESS).unwrap();
    let recipient = contract_address!("0x123");

    let chain_info = get_chain_info(&ChainId::Sepolia, None);
    let reader = RpcStateReader::new_with_config_from_url(rpc_url, chain_info, block_id);
    let balance_slot_low = get_storage_var_address("ERC20_balances", &[*dummy.0.key()]);
    let balance_slot_high = balance_slot_low.next_storage_key().unwrap();
    let balance_low = reader.get_storage_at(strk_token, balance_slot_low).unwrap();
    let balance_high = reader.get_storage_at(strk_token, balance_slot_high).unwrap();
    let nonce = reader.get_nonce_at(dummy).unwrap();

    let calldata =
        create_calldata(strk_token, "transfer", &[*recipient.key(), balance_low, balance_high]);

    invoke_tx(invoke_tx_args! {
        sender_address: dummy,
        calldata,
        resource_bounds: ValidResourceBounds::AllResources(resource_bounds_for_client_side_tx()),
        nonce,
    })
}

/// End-to-end signal that the storage-delete support added in
/// [`crate::running::storage_proofs`] works on real Sepolia data: drains DUMMY's STRK balance
/// via `transfer`, then runs the full virtual OS pipeline and asserts it completes.
///
/// **Manual regression check (recorded against this exact recording):** removing the
/// `compute_missing_sibling_keys` + `merge_storage_proofs` supplement-fetch wiring inside
/// `RpcStorageProofsProvider::get_storage_proofs` (i.e. reverting the storage-delete support
/// while keeping the validation lifted) makes the OS Cairo committer fail its internal
/// Patricia hash check with:
///
/// > `OsExecution(VirtualMachineError(InconsistentAutoDeduction((pedersen, ...))))`
///
/// because the placeholder `Binary(0, 0)` dummy node produces a wrong canonicalized
/// post-deletion hash.
///
/// ```bash
/// # Record (live Sepolia):
/// RECORD_RPC_RECORDS=1 NODE_URL=<sepolia-v0_10> \
///     cargo test -p starknet_transaction_prover test_run_virtual_os_with_storage_delete
///
/// # Offline (after recording):
/// cargo test -p starknet_transaction_prover test_run_virtual_os_with_storage_delete
/// ```
#[tokio::test(flavor = "multi_thread")]
async fn test_run_virtual_os_with_storage_delete() {
    let test_mode = resolve_test_mode("test_run_virtual_os_with_storage_delete").await;
    let rpc_url = test_mode.rpc_url();

    let invoke_tx = {
        let rpc_url = rpc_url.clone();
        tokio::task::spawn_blocking(move || {
            construct_drain_balance_invoke(rpc_url, BlockId::Latest)
        })
        .await
        .unwrap()
    };

    let factory = runner_factory(&rpc_url);
    let result = factory.run_virtual_os(BlockId::Latest, vec![invoke_tx]).await;
    result.expect("run_virtual_os should succeed");

    test_mode.finalize();
}
