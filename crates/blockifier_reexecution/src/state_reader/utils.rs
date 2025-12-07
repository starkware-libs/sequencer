use std::collections::{BTreeMap, HashMap};
use std::env;
use std::fs::{read_to_string, create_dir_all, write};
use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::{SystemTime, UNIX_EPOCH};

use apollo_gateway_config::config::RpcStateReaderConfig;
use apollo_rpc_execution::{ETH_FEE_CONTRACT_ADDRESS, STRK_FEE_CONTRACT_ADDRESS};
use assert_matches::assert_matches;
#[cfg(feature = "cairo_native")]
use apollo_compile_to_native_types::SierraCompilationConfig;
#[cfg(feature = "cairo_native")]
use blockifier::blockifier::config::ContractClassManagerConfig;
use blockifier::context::{ChainInfo, FeeTokenAddresses};
use blockifier::state::cached_state::{CachedState, CommitmentStateDiff, StateMaps};
use blockifier::state::state_api::StateReader;
#[cfg(feature = "cairo_native")]
use blockifier::state::state_reader_and_contract_manager::{
    FetchCompiledClasses,
    StateReaderAndContractManager,
};
use indexmap::IndexMap;
use pretty_assertions::assert_eq;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ChainId, ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_api::transaction::TransactionHash;
use starknet_api::{class_hash, contract_address};
use starknet_types_core::felt::Felt;

use crate::assert_eq_state_diff;
use crate::state_reader::cli::TransactionInput;
use crate::state_reader::errors::{ReexecutionError, ReexecutionResult};
use crate::state_reader::offline_state_reader::{
    OfflineConsecutiveStateReaders, SerializableDataPrevBlock, SerializableOfflineReexecutionData,
};
use crate::state_reader::reexecution_state_reader::{
    ConsecutiveReexecutionStateReaders, ReexecutionStateReader,
};
use crate::state_reader::serde_utils::deserialize_transaction_json_to_starknet_api_tx;
use crate::state_reader::test_state_reader::ConsecutiveTestStateReaders;

pub static RPC_NODE_URL: LazyLock<String> = LazyLock::new(|| {
    env::var("TEST_URL")
        .unwrap_or_else(|_| "https://free-rpc.nethermind.io/mainnet-juno/".to_string())
});

/// Returns the fee token addresses of mainnet.
pub fn get_fee_token_addresses(_chain_id: &ChainId) -> FeeTokenAddresses {
    let x = contract_address!("0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7");
    FeeTokenAddresses { strk_fee_token_address: x, eth_fee_token_address: x }
    // match chain_id {
    //     // Mainnet, testnet and integration systems have the same fee token addresses.
    //     ChainId::Mainnet | ChainId::Sepolia | ChainId::IntegrationSepolia => FeeTokenAddresses {
    //         strk_fee_token_address: *STRK_FEE_CONTRACT_ADDRESS,
    //         eth_fee_token_address: *ETH_FEE_CONTRACT_ADDRESS,
    //     },
    //     unknown_chain => unimplemented!("Unknown chain ID {unknown_chain}."),
    // }
}

/// Returns the RPC state reader configuration with the constant RPC_NODE_URL.
pub fn get_rpc_state_reader_config() -> RpcStateReaderConfig {
    RpcStateReaderConfig::from_url(RPC_NODE_URL.clone())
}

/// Returns the chain info of mainnet.
pub fn get_chain_info(chain_id: &ChainId) -> ChainInfo {
    ChainInfo {
        chain_id: chain_id.clone(),
        fee_token_addresses: get_fee_token_addresses(chain_id),
        is_l3: false,
    }
}

/// Creates a temporary directory for saving execution infos and returns its path.
fn create_execution_info_dir() -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let dir_path = PathBuf::from(format!("/tmp/blockifier_reexecution_execution_infos_{}", timestamp));
    create_dir_all(&dir_path).expect("Failed to create execution info directory");
    dir_path
}

/// Saves transaction execution info to a file.
fn save_execution_info(
    dir_path: &PathBuf,
    tx_index: usize,
    execution_info: &blockifier::transaction::objects::TransactionExecutionInfo,
) {
    let file_path = dir_path.join(format!("tx_{}.txt", tx_index));
    let content = format!("{:#?}", execution_info);
    write(&file_path, content)
        .unwrap_or_else(|e| panic!("Failed to write execution info to {:?}: {}", file_path, e));
}

/// Creates a ContractClassManagerConfig with custom native compilation settings for reexecution.
#[cfg(feature = "cairo_native")]
pub fn create_native_config_for_reexecution(
    run_cairo_native: bool,
    wait_on_native_compilation: bool,
) -> ContractClassManagerConfig {
    let native_compiler_config = SierraCompilationConfig {
        max_file_size: Some(52_428_800), // max_native_bytecode_size: 52428800
        max_cpu_time: Some(600),
        max_memory_usage: Some(16_106_127_360), // 16106127360 bytes
        optimization_level: 2,
        compiler_binary_path: None,
    };

    ContractClassManagerConfig {
        cairo_native_run_config: blockifier::blockifier::config::CairoNativeRunConfig {
            run_cairo_native,
            wait_on_native_compilation,
            ..Default::default()
        },
        native_compiler_config,
        ..Default::default()
    }
}

// TODO(Aner): import the following functions instead, to reduce code duplication.
pub(crate) fn disjoint_hashmap_union<K: std::hash::Hash + std::cmp::Eq, V>(
    map1: IndexMap<K, V>,
    map2: IndexMap<K, V>,
) -> IndexMap<K, V> {
    let expected_len = map1.len() + map2.len();
    let union_map: IndexMap<K, V> = map1.into_iter().chain(map2).collect();
    // verify union length is sum of lengths (disjoint union)
    assert_eq!(union_map.len(), expected_len, "Intersection of hashmaps is not empty.");
    union_map
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReexecutionStateMaps {
    nonces: HashMap<ContractAddress, Nonce>,
    class_hashes: HashMap<ContractAddress, ClassHash>,
    storage: HashMap<ContractAddress, HashMap<StorageKey, Felt>>,
    compiled_class_hashes: HashMap<ClassHash, CompiledClassHash>,
    declared_contracts: HashMap<ClassHash, bool>,
}

impl From<StateMaps> for ReexecutionStateMaps {
    fn from(value: StateMaps) -> Self {
        let mut storage: HashMap<ContractAddress, HashMap<StorageKey, Felt>> = HashMap::new();
        for ((address, key), v) in value.storage {
            match storage.get_mut(&address) {
                Some(entry) => {
                    entry.insert(key, v);
                }
                None => {
                    let mut entry = HashMap::new();
                    entry.insert(key, v);
                    storage.insert(address, entry);
                }
            }
        }
        ReexecutionStateMaps {
            nonces: value.nonces,
            class_hashes: value.class_hashes,
            storage,
            compiled_class_hashes: value.compiled_class_hashes,
            declared_contracts: value.declared_contracts,
        }
    }
}

impl TryFrom<ReexecutionStateMaps> for StateMaps {
    type Error = ReexecutionError;

    fn try_from(value: ReexecutionStateMaps) -> Result<Self, Self::Error> {
        let mut storage: HashMap<(ContractAddress, StorageKey), Felt> = HashMap::new();
        for (address, inner_map) in value.storage {
            for (key, v) in inner_map {
                storage.insert((address, key), v);
            }
        }
        Ok(StateMaps {
            nonces: value.nonces,
            class_hashes: value.class_hashes,
            storage,
            compiled_class_hashes: value.compiled_class_hashes,
            declared_contracts: value.declared_contracts,
        })
    }
}

#[macro_export]
macro_rules! retry_request {
    ($retry_config:expr, $closure:expr) => {{
        let mut attempt_number = 0;
        retry::retry(
            retry::delay::Fixed::from_millis($retry_config.retry_interval_milliseconds)
                .take($retry_config.n_retries),
            || {
                attempt_number += 1;
                match $closure() {
                    Ok(value) => retry::OperationResult::Ok(value),
                    // If the error contains any of the expected error strings, we want to retry.
                    Err(e)
                        if $retry_config
                            .expected_error_strings
                            .iter()
                            .any(|s| e.to_string().contains(s)) =>
                    {
                        println!(
                            "Attempt {}: Retrying request due to error: {:?}",
                            attempt_number, e
                        );
                        println!(
                            "Retry delay in milliseconds: {}",
                            $retry_config.retry_interval_milliseconds
                        );
                        retry::OperationResult::Retry(e)
                    }
                    // For all other errors, do not retry and return immediately.
                    Err(e) => retry::OperationResult::Err(e),
                }
            },
        )
        .map_err(|e| {
            if $retry_config.expected_error_strings.iter().any(|s| e.error.to_string().contains(s))
            {
                panic!("{}: {:?}", $retry_config.retry_failure_message, e.error);
            }
            e.error
        })
    }};
}

/// A struct for comparing `CommitmentStateDiff` instances, disregarding insertion order.
/// This struct converts `IndexMap` fields to `BTreeMap`, providing a consistent ordering of keys.
/// This allows `assert_eq` to produce clearer, ordered diffs when comparing instances, especially
/// useful in testing.
#[derive(Debug, PartialEq)]
pub struct ComparableStateDiff {
    address_to_class_hash: BTreeMap<ContractAddress, ClassHash>,
    address_to_nonce: BTreeMap<ContractAddress, Nonce>,
    storage_updates: BTreeMap<ContractAddress, BTreeMap<StorageKey, Felt>>,
    class_hash_to_compiled_class_hash: BTreeMap<ClassHash, CompiledClassHash>,
}

impl From<CommitmentStateDiff> for ComparableStateDiff {
    fn from(state_diff: CommitmentStateDiff) -> Self {
        // Use helper function to convert IndexMap to HashMap for simplicity
        fn to_btree_map<K: std::cmp::Ord, V>(index_map: IndexMap<K, V>) -> BTreeMap<K, V> {
            index_map.into_iter().collect()
        }

        ComparableStateDiff {
            address_to_class_hash: to_btree_map(state_diff.address_to_class_hash),
            address_to_nonce: to_btree_map(state_diff.address_to_nonce),
            storage_updates: state_diff
                .storage_updates
                .into_iter()
                .map(|(address, storage)| (address, to_btree_map(storage)))
                .collect(),
            class_hash_to_compiled_class_hash: to_btree_map(
                state_diff.class_hash_to_compiled_class_hash,
            ),
        }
    }
}

pub fn reexecute_and_verify_correctness<
    S: StateReader + Send + Sync + Clone + 'static,
    T: ConsecutiveReexecutionStateReaders<S>,
>(
    consecutive_state_readers: T,
) -> Option<CachedState<S>> {
    let expected_state_diff = consecutive_state_readers.get_next_block_state_diff().unwrap();
    tracing::info!("Got expected state diff");
    let all_txs_in_next_block = consecutive_state_readers.get_next_block_txs().unwrap();
    tracing::info!("Got all txs in next block");
    let mut transaction_executor =
        consecutive_state_readers.pre_process_and_create_executor(None).unwrap();
    tracing::info!("Created transaction executor");
    let execution_results = transaction_executor.execute_txs(&all_txs_in_next_block, None);
    
    // Create directory for saving execution infos
    let exec_info_dir = create_execution_info_dir();
    println!("Saving execution infos to: {}", exec_info_dir.display());
    
    // Verify all transactions executed successfully and save execution infos.
    for (idx, res) in execution_results.iter().enumerate() {
        assert_matches!(res, Ok(_));
        let (tx_execution_info, _state_maps) = res.as_ref().unwrap();
        save_execution_info(&exec_info_dir, idx, tx_execution_info);
    }

    // Finalize block and read actual statediff; using non_consuming_finalize to keep the
    // block_state.
    let actual_state_diff =
        transaction_executor.non_consuming_finalize().expect("Couldn't finalize block").state_diff;

    assert_eq_state_diff!(expected_state_diff, actual_state_diff);

    transaction_executor.block_state
}

/// Trait for types that support creating an executor with Cairo native execution.
#[cfg(feature = "cairo_native")]
pub trait ConsecutiveReexecutionStateReadersWithNative<
    S: StateReader + FetchCompiledClasses + Send + Sync + Clone + 'static,
>: ConsecutiveReexecutionStateReaders<S>
{
    fn pre_process_and_create_executor_with_native(
        self,
        contract_class_manager_config: ContractClassManagerConfig,
    ) -> ReexecutionResult<
        blockifier::blockifier::transaction_executor::TransactionExecutor<
            StateReaderAndContractManager<S>,
        >,
    >;
}

/// Counts the number of calls executed with Cairo native in a CallInfo and its inner calls.
/// Returns (native_count, total_count).
#[cfg(feature = "cairo_native")]
fn count_native_calls(call_info: &blockifier::execution::call_info::CallInfo) -> (usize, usize) {
    let mut native_count = 0;
    let mut total_count = 0;

    for call in call_info.iter() {
        total_count += 1;
        if call.execution.cairo_native {
            native_count += 1;
        }
    }

    (native_count, total_count)
}

/// Reexecutes transactions and verifies correctness using Cairo native execution.
#[cfg(feature = "cairo_native")]
pub fn reexecute_and_verify_correctness_with_native<
    S: StateReader + FetchCompiledClasses + Send + Sync + Clone + 'static,
    T: ConsecutiveReexecutionStateReadersWithNative<S>,
>(
    consecutive_state_readers: T,
    contract_class_manager_config: ContractClassManagerConfig,
) -> Option<CachedState<StateReaderAndContractManager<S>>> {
    let expected_state_diff = consecutive_state_readers.get_next_block_state_diff().unwrap();
    tracing::info!("Got expected state diff");
    let all_txs_in_next_block = consecutive_state_readers.get_next_block_txs().unwrap();
    tracing::info!("Got all txs in next block");
    let mut transaction_executor = consecutive_state_readers
        .pre_process_and_create_executor_with_native(contract_class_manager_config)
        .unwrap();
    tracing::info!("Created transaction executor with Cairo native");

    println!(
        "Executing {} transactions with Cairo native (run_cairo_native=true, \
         wait_on_native_compilation=true)...",
        all_txs_in_next_block.len()
    );
    println!("{}", "=".repeat(80));

    let execution_results = transaction_executor.execute_txs(&all_txs_in_next_block, None);

    // Create directory for saving execution infos
    let exec_info_dir = create_execution_info_dir();
    println!("Saving execution infos to: {}", exec_info_dir.display());

    let mut total_native_calls = 0;
    let mut total_calls = 0;

    // Verify all transactions executed successfully and print native call statistics.
    for (idx, res) in execution_results.iter().enumerate() {
        assert_matches!(res, Ok(_));
        let (tx_execution_info, _state_maps) = res.as_ref().unwrap();

        // Save execution info to file
        save_execution_info(&exec_info_dir, idx, tx_execution_info);

        // Count native calls across all call infos in this transaction.
        let mut tx_native_calls = 0;
        let mut tx_total_calls = 0;

        if let Some(ref call_info) = tx_execution_info.validate_call_info {
            let (native, total) = count_native_calls(call_info);
            tx_native_calls += native;
            tx_total_calls += total;
        }
        if let Some(ref call_info) = tx_execution_info.execute_call_info {
            let (native, total) = count_native_calls(call_info);
            tx_native_calls += native;
            tx_total_calls += total;
        }
        if let Some(ref call_info) = tx_execution_info.fee_transfer_call_info {
            let (native, total) = count_native_calls(call_info);
            tx_native_calls += native;
            tx_total_calls += total;
        }

        total_native_calls += tx_native_calls;
        total_calls += tx_total_calls;

        println!(
            "Transaction {}/{}: {} native calls out of {} total calls ({:.1}% native)",
            idx + 1,
            all_txs_in_next_block.len(),
            tx_native_calls,
            tx_total_calls,
            if tx_total_calls > 0 {
                (tx_native_calls as f64 / tx_total_calls as f64) * 100.0
            } else {
                0.0
            }
        );
    }

    println!("{}", "=".repeat(80));
    println!(
        "All {} transactions executed successfully with Cairo native.",
        all_txs_in_next_block.len()
    );
    println!(
        "Total: {} native calls out of {} total calls ({:.1}% native)",
        total_native_calls,
        total_calls,
        if total_calls > 0 {
            (total_native_calls as f64 / total_calls as f64) * 100.0
        } else {
            0.0
        }
    );
    println!("{}", "=".repeat(80));

    // Finalize block and read actual statediff; using non_consuming_finalize to keep the
    // block_state.
    let actual_state_diff =
        transaction_executor.non_consuming_finalize().expect("Couldn't finalize block").state_diff;

    assert_eq_state_diff!(expected_state_diff, actual_state_diff);

    transaction_executor.block_state
}

pub fn reexecute_block_for_testing(block_number: u64) {
    // In tests we are already in the blockifier_reexecution directory.
    let full_file_path = format!("./resources/block_{block_number}/reexecution_data.json");

    reexecute_and_verify_correctness(
        OfflineConsecutiveStateReaders::new_from_file(&full_file_path).unwrap(),
    );

    println!("Reexecution test for block {block_number} passed successfully.");
}

pub fn write_block_reexecution_data_to_file(
    block_number: BlockNumber,
    full_file_path: String,
    node_url: String,
    chain_id: ChainId,
) {
    tracing::info!("Writing reexecution data to file for block {block_number}");
    let config = RpcStateReaderConfig::from_url(node_url);
    tracing::info!("Got RPC state reader config");

    let consecutive_state_readers = ConsecutiveTestStateReaders::new(
        block_number.prev().expect("Should not run with block 0"),
        Some(config),
        chain_id.clone(),
        true,
    );

    tracing::info!("Got consecutive state readers");
    consecutive_state_readers
        .last_block_state_reader
        .get_class_hash_at(contract_address!(
            "0x049d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7"
        ))
        .unwrap();
    consecutive_state_readers
        .next_block_state_reader
        .get_class_hash_at(contract_address!(
            "0x049d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7"
        ))
        .unwrap();
    consecutive_state_readers
        .last_block_state_reader
        .get_compiled_class(class_hash!(
            "0xd0e183745e9dae3e4e78a8ffedcce0903fc4900beace4e0abf192d4c202da3"
        ))
        .unwrap();
    consecutive_state_readers
        .next_block_state_reader
        .get_compiled_class(class_hash!(
            "0xd0e183745e9dae3e4e78a8ffedcce0903fc4900beace4e0abf192d4c202da3"
        ))
        .unwrap();

    let serializable_data_next_block =
        consecutive_state_readers.get_serializable_data_next_block().unwrap();
    tracing::info!("Got serializable data next block");
    let old_block_hash = consecutive_state_readers.get_old_block_hash().unwrap();

    // Run the reexecution test and get the state maps and contract class mapping.
    let block_state = reexecute_and_verify_correctness(consecutive_state_readers).unwrap();
    let serializable_data_prev_block = SerializableDataPrevBlock {
        state_maps: block_state.get_initial_reads().unwrap().into(),
        contract_class_mapping: block_state.state.get_contract_class_mapping_dumper().unwrap(),
    };

    // Write the reexecution data to a json file.
    SerializableOfflineReexecutionData {
        serializable_data_prev_block,
        serializable_data_next_block,
        chain_id,
        old_block_hash,
    }
    .write_to_file(&full_file_path)
    .unwrap();

    println!("RPC replies required for reexecuting block {block_number} written to json file.");
}

/// Executes a single transaction from a JSON file or given a transaction hash, using RPC to fetch
/// block context. Does not assert correctness, only prints the execution result.
pub fn execute_single_transaction(
    block_number: BlockNumber,
    node_url: String,
    chain_id: ChainId,
    tx_input: TransactionInput,
    run_cairo_native: bool,
) -> ReexecutionResult<()> {
    // Create RPC config.
    let config = RpcStateReaderConfig::from_url(node_url);

    // Create ConsecutiveTestStateReaders first.
    let consecutive_state_readers = ConsecutiveTestStateReaders::new(
        block_number.prev().expect("Should not run with block 0"),
        Some(config),
        chain_id.clone(),
        false, // dump_mode = false
    );

    // Get transaction and hash based on input method.
    let (transaction, transaction_hash) = match tx_input {
        TransactionInput::FromHash { tx_hash } => {
            // Fetch transaction from the next block (the block containing the transaction to
            // execute).
            let transaction =
                consecutive_state_readers.next_block_state_reader.get_tx_by_hash(&tx_hash)?;
            let transaction_hash = TransactionHash(Felt::from_hex_unchecked(&tx_hash));

            (transaction, transaction_hash)
        }
        TransactionInput::FromFile { tx_path } => {
            // Load the transaction from a local JSON file.
            let json_content = read_to_string(&tx_path)
                .unwrap_or_else(|_| panic!("Failed to read transaction JSON file: {}.", tx_path));
            let json_value: Value = serde_json::from_str(&json_content)?;
            let transaction = deserialize_transaction_json_to_starknet_api_tx(json_value)?;
            let transaction_hash = transaction.calculate_transaction_hash(&chain_id)?;

            (transaction, transaction_hash)
        }
    };

    // Convert transaction to BlockifierTransaction using api_txs_to_blockifier_txs_next_block.
    let blockifier_tx = consecutive_state_readers
        .next_block_state_reader
        .api_txs_to_blockifier_txs_next_block(vec![(transaction, transaction_hash)])?;

    #[cfg(feature = "cairo_native")]
    if run_cairo_native {
        use crate::state_reader::utils::ConsecutiveReexecutionStateReadersWithNative;

        println!(
            "Executing transaction with Cairo native (run_cairo_native=true, \
             wait_on_native_compilation=true)..."
        );
        println!("{}", "=".repeat(80));

        // Create transaction executor with Cairo native support.
        let contract_class_manager_config = create_native_config_for_reexecution(true, true);
        let mut transaction_executor = consecutive_state_readers
            .pre_process_and_create_executor_with_native(contract_class_manager_config)?;

        // Execute transaction (should be single element).
        let execution_results = transaction_executor.execute_txs(&blockifier_tx, None);

        // We expect exactly one execution result since we executed a single transaction.
        let res =
            execution_results.first().expect("Expected exactly one execution result, but got none");

        println!("Transaction executed successfully with Cairo native.");
        let (tx_execution_info, _state_maps) = res.as_ref().unwrap();

        // Count native calls across all call infos in this transaction.
        let mut tx_native_calls = 0;
        let mut tx_total_calls = 0;

        if let Some(ref call_info) = tx_execution_info.validate_call_info {
            let (native, total) = count_native_calls(call_info);
            tx_native_calls += native;
            tx_total_calls += total;
        }
        if let Some(ref call_info) = tx_execution_info.execute_call_info {
            let (native, total) = count_native_calls(call_info);
            tx_native_calls += native;
            tx_total_calls += total;
        }
        if let Some(ref call_info) = tx_execution_info.fee_transfer_call_info {
            let (native, total) = count_native_calls(call_info);
            tx_native_calls += native;
            tx_total_calls += total;
        }

        println!(
            "Native calls: {} out of {} total calls ({:.1}% native)",
            tx_native_calls,
            tx_total_calls,
            if tx_total_calls > 0 {
                (tx_native_calls as f64 / tx_total_calls as f64) * 100.0
            } else {
                0.0
            }
        );
        println!("{}", "=".repeat(80));

        return Ok(());
    }

    #[cfg(not(feature = "cairo_native"))]
    if run_cairo_native {
        panic!("Cairo native feature is not enabled. Rebuild with --features cairo_native");
    }

    // Create transaction executor.
    let mut transaction_executor =
        consecutive_state_readers.pre_process_and_create_executor(None)?;

    // Execute transaction (should be single element).
    let execution_results = transaction_executor.execute_txs(&blockifier_tx, None);

    // We expect exactly one execution result since we executed a single transaction.
    let res =
        execution_results.first().expect("Expected exactly one execution result, but got none");

    println!("Execution result: {:?}", res);

    Ok(())
}

/// Asserts equality between two `CommitmentStateDiff` structs, ignoring insertion order.
#[macro_export]
macro_rules! assert_eq_state_diff {
    ($expected_state_diff:expr, $actual_state_diff:expr $(,)?) => {
        pretty_assertions::assert_eq!(
            $crate::state_reader::utils::ComparableStateDiff::from($expected_state_diff,),
            $crate::state_reader::utils::ComparableStateDiff::from($actual_state_diff,),
            "Expected and actual state diffs do not match.",
        );
    };
}
