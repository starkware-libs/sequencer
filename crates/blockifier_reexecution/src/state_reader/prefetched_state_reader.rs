//! State reader backed by prefetched [`StateMaps`] from `starknet_simulateTransactions` with
//! `RETURN_INITIAL_READS`.

use blockifier::execution::contract_class::RunnableCompiledClass;
use blockifier::state::cached_state::StateMaps;
use blockifier::state::global_cache::CompiledClasses;
use blockifier::state::state_api::{StateReader, StateResult};
use blockifier::state::state_reader_and_contract_manager::FetchCompiledClasses;
use serde::Deserialize;
use serde_json::json;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::rpc_transaction::{RpcInvokeTransaction, RpcInvokeTransactionV3, RpcTransaction};
use starknet_api::state::StorageKey;
use starknet_api::transaction::{InvokeTransaction, Transaction, TransactionHash};
use starknet_core::types::ContractClass as StarknetContractClass;
use starknet_types_core::felt::Felt;

use crate::errors::{ReexecutionError, ReexecutionResult};
use crate::state_reader::reexecution_state_reader::ReexecutionStateReader;
use crate::state_reader::rpc_objects::BlockId;
use crate::state_reader::rpc_state_reader::RpcStateReader;

/// State reader backed by prefetched [`StateMaps`] from simulate.
///
/// Serves storage, nonce, class hash, and declared contract reads from the prefetched state.
/// Falls back to the inner [`RpcStateReader`] when a key is missing from the prefetched state
/// (e.g., when simulate uses different flags than execution).
/// Delegates compiled class lookups to the inner [`RpcStateReader`] since simulate responses
/// do not include classes.
pub struct SimulatedStateReader {
    state_maps: StateMaps,
    rpc_state_reader: RpcStateReader,
}

impl SimulatedStateReader {
    pub fn new(state_maps: StateMaps, rpc_state_reader: RpcStateReader) -> Self {
        Self { state_maps, rpc_state_reader }
    }
}

impl StateReader for SimulatedStateReader {
    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<Felt> {
        match self.state_maps.storage.get(&(contract_address, key)) {
            Some(value) => Ok(*value),
            None => self.rpc_state_reader.get_storage_at(contract_address, key),
        }
    }

    fn get_nonce_at(&self, contract_address: ContractAddress) -> StateResult<Nonce> {
        match self.state_maps.nonces.get(&contract_address) {
            Some(value) => Ok(*value),
            None => self.rpc_state_reader.get_nonce_at(contract_address),
        }
    }

    fn get_class_hash_at(&self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        match self.state_maps.class_hashes.get(&contract_address) {
            Some(value) => Ok(*value),
            None => self.rpc_state_reader.get_class_hash_at(contract_address),
        }
    }

    fn get_compiled_class(&self, class_hash: ClassHash) -> StateResult<RunnableCompiledClass> {
        self.rpc_state_reader.get_compiled_class(class_hash)
    }

    fn get_compiled_class_hash(&self, class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        self.rpc_state_reader.get_compiled_class_hash(class_hash)
    }

    fn get_compiled_class_hash_v2(
        &self,
        class_hash: ClassHash,
        compiled_class: &RunnableCompiledClass,
    ) -> StateResult<CompiledClassHash> {
        self.rpc_state_reader.get_compiled_class_hash_v2(class_hash, compiled_class)
    }
}

impl FetchCompiledClasses for SimulatedStateReader {
    fn get_compiled_classes(&self, class_hash: ClassHash) -> StateResult<CompiledClasses> {
        self.rpc_state_reader.get_compiled_classes(class_hash)
    }

    fn is_declared(&self, class_hash: ClassHash) -> StateResult<bool> {
        match self.state_maps.declared_contracts.get(&class_hash) {
            Some(value) => Ok(*value),
            None => self.rpc_state_reader.is_declared(class_hash),
        }
    }
}

/// Calls `starknet_simulateTransactions` with `RETURN_INITIAL_READS` and returns the
/// initial state reads as `StateMaps`.
///
/// Requires a v0.10+ node that supports the `RETURN_INITIAL_READS` flag.
pub fn simulate_and_get_initial_reads(
    rpc_state_reader: &RpcStateReader,
    block_id: BlockId,
    txs: &[(InvokeTransaction, TransactionHash)],
    validate_txs: bool,
    skip_fee_charge: bool,
) -> ReexecutionResult<StateMaps> {
    let rpc_txs: Vec<RpcTransaction> = txs
        .iter()
        .map(|(tx, _)| match tx {
            InvokeTransaction::V3(v3) => RpcInvokeTransactionV3::try_from(v3.clone())
                .map(RpcInvokeTransaction::V3)
                .map(RpcTransaction::Invoke)
                .map_err(|e| ReexecutionError::PrefetchState(e.to_string())),
            _ => Err(ReexecutionError::PrefetchState(
                "Only Invoke V3 transactions are supported for simulate".to_string(),
            )),
        })
        .collect::<Result<Vec<_>, _>>()?;

    // Build simulation flags that match execution behavior as closely as possible.
    // Mismatches cause prefetch cache misses (handled by RPC fallback) but hurt
    // performance.
    let mut simulation_flags = vec!["RETURN_INITIAL_READS"];
    if !validate_txs {
        simulation_flags.push("SKIP_VALIDATE");
    }
    if skip_fee_charge {
        simulation_flags.push("SKIP_FEE_CHARGE");
    }

    let params = json!({
        "block_id": block_id,
        "transactions": rpc_txs,
        "simulation_flags": simulation_flags
    });

    let result = rpc_state_reader.send_rpc_request("starknet_simulateTransactions", params)?;

    let initial_reads_value = result.get("initial_reads").cloned().ok_or_else(|| {
        ReexecutionError::PrefetchState(
            "simulateTransactions response missing initial_reads (ensure RETURN_INITIAL_READS and \
             v0.10 endpoint)"
                .to_string(),
        )
    })?;

    deserialize_rpc_initial_reads(initial_reads_value).map_err(|e| {
        ReexecutionError::PrefetchState(format!("Failed to deserialize initial_reads: {e}"))
    })
}

// `initial_reads` response shape from `starknet_simulateTransactions` (Starknet spec v0.12).
#[derive(Deserialize)]
struct StorageEntry {
    contract_address: ContractAddress,
    key: StorageKey,
    value: Felt,
}

#[derive(Deserialize)]
struct NonceEntry {
    contract_address: ContractAddress,
    nonce: Nonce,
}

#[derive(Deserialize)]
struct ClassHashEntry {
    contract_address: ContractAddress,
    class_hash: ClassHash,
}

#[derive(Deserialize)]
struct DeclaredEntry {
    class_hash: ClassHash,
    is_declared: bool,
}

#[derive(Deserialize, Default)]
struct RpcInitialReads {
    #[serde(default)]
    storage: Vec<StorageEntry>,
    #[serde(default)]
    nonces: Vec<NonceEntry>,
    #[serde(default)]
    class_hashes: Vec<ClassHashEntry>,
    #[serde(default)]
    declared_contracts: Vec<DeclaredEntry>,
}

impl From<RpcInitialReads> for StateMaps {
    fn from(r: RpcInitialReads) -> Self {
        let mut m = StateMaps::default();
        m.storage.extend(r.storage.into_iter().map(|e| ((e.contract_address, e.key), e.value)));
        m.nonces.extend(r.nonces.into_iter().map(|e| (e.contract_address, e.nonce)));
        m.class_hashes
            .extend(r.class_hashes.into_iter().map(|e| (e.contract_address, e.class_hash)));
        m.declared_contracts
            .extend(r.declared_contracts.into_iter().map(|e| (e.class_hash, e.is_declared)));
        m
    }
}

/// Deserializes `initial_reads` JSON from `starknet_simulateTransactions` into blockifier
/// `StateMaps`.
pub fn deserialize_rpc_initial_reads(value: serde_json::Value) -> Result<StateMaps, String> {
    serde_json::from_value::<RpcInitialReads>(value).map(Into::into).map_err(|e| e.to_string())
}

impl SimulatedStateReader {
    /// Creates a `SimulatedStateReader` by simulating the invoke V3 transactions in `txs`
    /// to prefetch their initial state reads. Non-V3 transactions are ignored (their state
    /// reads will fall back to individual RPC calls).
    pub fn from_rpc_state_reader(
        rpc_state_reader: RpcStateReader,
        txs: &[(Transaction, TransactionHash)],
        skip_fee_charge: bool,
    ) -> ReexecutionResult<Self> {
        let invoke_v3_txs: Vec<_> = txs
            .iter()
            .filter_map(|(tx, hash)| match tx {
                Transaction::Invoke(invoke @ InvokeTransaction::V3(_)) => {
                    Some((invoke.clone(), *hash))
                }
                _ => None,
            })
            .collect();
        let state_maps = if invoke_v3_txs.is_empty() {
            StateMaps::default()
        } else {
            // Skip validation in simulate — this is only for prefetching state reads.
            let validate_txs = false;
            simulate_and_get_initial_reads(
                &rpc_state_reader,
                rpc_state_reader.block_id,
                &invoke_v3_txs,
                validate_txs,
                skip_fee_charge,
            )?
        };
        Ok(Self { state_maps, rpc_state_reader })
    }
}

impl ReexecutionStateReader for SimulatedStateReader {
    fn get_contract_class(&self, class_hash: &ClassHash) -> StateResult<StarknetContractClass> {
        self.rpc_state_reader.get_contract_class(class_hash)
    }

    fn get_old_block_hash(&self, old_block_number: BlockNumber) -> ReexecutionResult<BlockHash> {
        self.rpc_state_reader.get_old_block_hash(old_block_number)
    }
}
