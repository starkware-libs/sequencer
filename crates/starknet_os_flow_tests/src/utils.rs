use std::collections::HashMap;

use assert_matches::assert_matches;
use blockifier::abi::constants::STORED_BLOCK_HASH_BUFFER;
use blockifier::blockifier::config::TransactionExecutorConfig;
use blockifier::blockifier::transaction_executor::{
    TransactionExecutionOutput,
    TransactionExecutor,
};
use blockifier::context::BlockContext;
use blockifier::state::cached_state::{CachedState, StateMaps};
use blockifier::state::state_api::StateReader;
use blockifier::test_utils::contracts::FeatureContractTrait;
use blockifier::transaction::transaction_execution::Transaction;
use blockifier_test_utils::cairo_versions::CairoVersion;
use blockifier_test_utils::contracts::FeatureContract;
use starknet_api::block::{BlockHash, BlockHashAndNumber, BlockNumber};
use starknet_api::contract_class::compiled_class_hash::HashVersion;
use starknet_api::contract_class::{ClassInfo, ContractClass, SierraVersion};
use starknet_api::core::{ClassHash, ContractAddress, Nonce, PatriciaKey};
use starknet_api::declare_tx_args;
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::executable_transaction::{AccountTransaction, DeclareTransaction};
use starknet_api::test_utils::declare::declare_tx;
use starknet_api::test_utils::{test_block_hash, NonceManager, CHAIN_ID_FOR_TESTS};
use starknet_api::transaction::fields::{Fee, ValidResourceBounds};
use starknet_api::transaction::TransactionVersion;
use starknet_committer::block_committer::input::{StarknetStorageKey, StarknetStorageValue};
use starknet_os::hints::hint_implementation::deprecated_compiled_class::class_hash::compute_deprecated_class_hash;
use starknet_os::hints::vars::Const;
use starknet_types_core::felt::Felt;

use crate::initial_state::OsExecutionContracts;
use crate::test_manager::FUNDED_ACCOUNT_ADDRESS;
use crate::tests::NON_TRIVIAL_RESOURCE_BOUNDS;

pub(crate) struct ExecutionOutput<S: StateReader + Send> {
    pub(crate) execution_outputs: Vec<TransactionExecutionOutput>,
    pub(crate) final_state: CachedState<S>,
}

/// Executes the given transactions on the given state and block context with default execution
/// configuration.
pub(crate) fn execute_transactions<S: StateReader + Send>(
    initial_state: S,
    txs: &[Transaction],
    block_context: BlockContext,
    virtual_os: bool,
) -> ExecutionOutput<S> {
    let block_number_hash_pair =
        maybe_dummy_block_hash_and_number(block_context.block_info().block_number);
    let config = TransactionExecutorConfig::default();

    // Execute.
    let mut executor = if virtual_os {
        // In virtual OS mode, the executor is created without preprocessing.
        TransactionExecutor::new(CachedState::new(initial_state), block_context, config)
    } else {
        TransactionExecutor::pre_process_and_create(
            initial_state,
            block_context,
            block_number_hash_pair.map(|(number, hash)| BlockHashAndNumber { hash, number }),
            config,
        )
        .expect("Failed to create transaction executor.")
    };

    // Execute the transactions and make sure none of them failed.
    let execution_deadline = None;
    let execution_results = executor
        .execute_txs_sequentially(txs, execution_deadline)
        .into_iter()
        .collect::<Vec<Result<_, _>>>();
    let mut execution_outputs = Vec::new();
    for (tx_index, result) in execution_results.into_iter().enumerate() {
        match result {
            Ok(output) => execution_outputs.push(output),
            Err(error) => {
                panic!("Unexpected error during execution of tx at index {tx_index}: {error}.");
            }
        }
    }

    if !virtual_os {
        // Finalize the block.
        executor.finalize().expect("Failed to finalize block.");
    }

    let final_state = executor.block_state.unwrap();
    ExecutionOutput { execution_outputs, final_state }
}

pub(crate) fn create_cairo1_bootstrap_declare_tx(
    feature_contract: FeatureContract,
    execution_contracts: &mut OsExecutionContracts,
) -> AccountTransaction {
    assert_matches!(feature_contract.cairo_version(), CairoVersion::Cairo1(_));
    create_declare_tx(feature_contract, &mut NonceManager::default(), execution_contracts, true)
}

/// Create a declare tx from the funded account. Optionally creates the tx as a bootstrap-declare.
pub(crate) fn create_declare_tx(
    feature_contract: FeatureContract,
    nonce_manager: &mut NonceManager,
    execution_contracts: &mut OsExecutionContracts,
    bootstrap: bool,
) -> AccountTransaction {
    let sender_address =
        if bootstrap { DeclareTransaction::bootstrap_address() } else { *FUNDED_ACCOUNT_ADDRESS };
    let nonce = if bootstrap { Nonce::default() } else { nonce_manager.next(sender_address) };
    let declare_args = match feature_contract.get_class() {
        ContractClass::V0(class) => {
            let class_hash = ClassHash(compute_deprecated_class_hash(&class).unwrap());
            execution_contracts.add_deprecated_contract(class_hash, class);
            declare_tx_args! {
                version: TransactionVersion::ONE,
                max_fee: if bootstrap { Fee::default() } else { Fee(1_000_000_000_000_000) },
                class_hash,
                sender_address,
                nonce,
            }
        }
        ContractClass::V1((casm, _sierra_version)) => {
            let sierra = feature_contract.get_sierra();
            let class_hash = sierra.calculate_class_hash();
            let compiled_class_hash = feature_contract.get_compiled_class_hash(&HashVersion::V2);
            execution_contracts.add_cairo1_contract(casm, &sierra);
            declare_tx_args! {
                sender_address,
                class_hash,
                compiled_class_hash,
                resource_bounds: if bootstrap {
                    ValidResourceBounds::create_for_testing_no_fee_enforcement()
                } else {
                    *NON_TRIVIAL_RESOURCE_BOUNDS
                },
                nonce,
            }
        }
    };
    let account_declare_tx = declare_tx(declare_args);
    let class_info = get_class_info_of_feature_contract(feature_contract);
    let tx =
        DeclareTransaction::create(account_declare_tx, class_info, &CHAIN_ID_FOR_TESTS).unwrap();
    AccountTransaction::Declare(tx)
}

/// Gets the extended initial reads for the OS run.
/// The extended initial reads are the initial reads with the class hash and nonce of each accessed
/// contract.
pub(crate) fn get_extended_initial_reads<S: StateReader>(state: &CachedState<S>) -> StateMaps {
    let raw_initial_reads = state.get_initial_reads().unwrap();
    // Populate the state initial reads with the class hash and nonce of each accessed contract.
    for contract_address in raw_initial_reads.get_contract_addresses() {
        state.get_class_hash_at(contract_address).unwrap();
        state.get_nonce_at(contract_address).unwrap();
    }

    for class_hash in raw_initial_reads.declared_contracts.keys() {
        state.get_compiled_class_hash(*class_hash).unwrap();
    }

    // Take the initial reads again to get the updated initial reads.
    let mut extended_initial_reads = state.get_initial_reads().unwrap();
    // This field is not used by the OS, so we clear it.
    extended_initial_reads.declared_contracts.clear();
    extended_initial_reads
}

pub(crate) fn maybe_dummy_block_hash_and_number(
    block_number: BlockNumber,
) -> Option<(BlockNumber, BlockHash)> {
    if block_number.0 < STORED_BLOCK_HASH_BUFFER {
        return None;
    }
    let old_block_number = block_number.0 - STORED_BLOCK_HASH_BUFFER;
    let block_hash = test_block_hash(old_block_number);
    Some((BlockNumber(old_block_number), block_hash))
}

pub(crate) fn divide_vec_into_n_parts<T>(mut vec: Vec<T>, n: usize) -> Vec<Vec<T>> {
    assert!(n > 0, "Number of parts must be positive");
    let minimal_items_per_part = vec.len() / n;
    let remainder = vec.len() % n;
    let mut items_per_part = Vec::with_capacity(n);
    for i in 0..n {
        let part_len = minimal_items_per_part + usize::from(i < remainder);
        let part: Vec<T> = vec.drain(0..part_len).collect();
        items_per_part.push(part);
    }
    assert_eq!(n, items_per_part.len(), "Number of parts does not match.");
    items_per_part
}

pub(crate) fn get_class_info_of_cairo0_contract(
    contract_class: DeprecatedContractClass,
) -> ClassInfo {
    let abi_length = contract_class.abi.as_ref().unwrap().len();
    ClassInfo {
        contract_class: ContractClass::V0(contract_class),
        sierra_program_length: 0,
        abi_length,
        sierra_version: SierraVersion::DEPRECATED,
    }
}

pub(crate) fn get_class_info_of_feature_contract(feature_contract: FeatureContract) -> ClassInfo {
    match feature_contract.get_class() {
        ContractClass::V0(contract_class) => get_class_info_of_cairo0_contract(contract_class),
        ContractClass::V1((contract_class, sierra_version)) => {
            let sierra = feature_contract.get_sierra();
            ClassInfo {
                contract_class: ContractClass::V1((contract_class, sierra_version.clone())),
                sierra_program_length: sierra.sierra_program.len(),
                abi_length: sierra.abi.len(),
                sierra_version,
            }
        }
    }
}

pub(crate) fn get_class_hash_of_feature_contract(feature_contract: FeatureContract) -> ClassHash {
    match feature_contract.get_class() {
        ContractClass::V0(class) => ClassHash(compute_deprecated_class_hash(&class).unwrap()),
        ContractClass::V1(_) => feature_contract.get_sierra().calculate_class_hash(),
    }
}

/// Utility method to update a map of expected storage updates.
pub(crate) fn update_expected_storage(
    expected_storage_updates: &mut HashMap<
        ContractAddress,
        HashMap<StarknetStorageKey, StarknetStorageValue>,
    >,
    address: ContractAddress,
    key: Felt,
    value: Felt,
) {
    let key = StarknetStorageKey::try_from(key).unwrap();
    let value = StarknetStorageValue(value);
    expected_storage_updates
        .entry(address)
        .and_modify(|map| {
            map.insert(key, value);
        })
        .or_insert_with(|| HashMap::from([(key, value)]));
}

/// Given the first block number in the multiblock and the number of blocks in the multiblock,
/// update the expected storage updates for the block hash contract.
pub(crate) fn update_expected_storage_updates_for_block_hash_contract(
    expected_storage_updates: &mut HashMap<
        ContractAddress,
        HashMap<StarknetStorageKey, StarknetStorageValue>,
    >,
    first_block_number: BlockNumber,
    n_blocks_in_multi_block: usize,
) {
    // The OS is expected to write the (number -> hash) mapping of this block. Make sure the current
    // block number is greater than STORED_BLOCK_HASH_BUFFER.
    let old_block_number = first_block_number.0 - STORED_BLOCK_HASH_BUFFER;
    assert!(
        old_block_number > 0,
        "Block number must be big enough to test a non-trivial block hash mapping update."
    );

    // Add old block hashes to expected storage updates.
    let block_hash_contract_address = ContractAddress(
        PatriciaKey::try_from(Const::BlockHashContractAddress.fetch_from_os_program().unwrap())
            .unwrap(),
    );
    for block_number in first_block_number.0
        ..(first_block_number.0 + u64::try_from(n_blocks_in_multi_block).unwrap())
    {
        let (old_block_number, old_block_hash) =
            maybe_dummy_block_hash_and_number(BlockNumber(block_number)).unwrap();
        update_expected_storage(
            expected_storage_updates,
            block_hash_contract_address,
            Felt::from(old_block_number.0),
            old_block_hash.0,
        );
    }
}
