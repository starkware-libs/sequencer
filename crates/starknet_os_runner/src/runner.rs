use std::collections::{BTreeMap, HashMap};

use blockifier::state::contract_class_manager::ContractClassManager;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::block_hash::block_hash_calculator::BlockHeaderCommitments;
use starknet_api::transaction::{InvokeTransaction, TransactionHash};
use starknet_os::io::os_input::{
    OsBlockInput,
    OsChainInfo,
    OsHints,
    OsHintsConfig,
    StarknetOsInput,
};
use starknet_os::io::os_output::StarknetOsRunnerOutput;
use starknet_os::runner::{run_os_stateless, DEFAULT_OS_LAYOUT};

use crate::classes_provider::ClassesProvider;
use crate::errors::RunnerError;
use crate::storage_proofs::StorageProofProvider;
use crate::virtual_block_executor::VirtualBlockExecutor;

pub struct Runner<C, S, V>
where
    C: ClassesProvider,
    S: StorageProofProvider,
    V: VirtualBlockExecutor,
{
    pub classes_provider: C,
    pub storage_proofs_provider: S,
    pub virtual_block_executor: V,
}

impl<C, S, V> Runner<C, S, V>
where
    C: ClassesProvider,
    S: StorageProofProvider,
    V: VirtualBlockExecutor,
{
    pub fn new(classes_provider: C, storage_proofs_provider: S, virtual_block_executor: V) -> Self {
        Self { classes_provider, storage_proofs_provider, virtual_block_executor }
    }

    /// Creates the OS hints required to run the given transactions virtually
    /// on top of the given block number.
    pub fn create_os_hints(
        &self,
        block_number: BlockNumber,
        contract_class_manager: ContractClassManager,
        txs: Vec<(InvokeTransaction, TransactionHash)>,
    ) -> Result<OsHints, RunnerError> {
        // 1. Execute virtual block and get execution data.
        let mut execution_data = self.virtual_block_executor.execute(
            block_number,
            contract_class_manager.clone(),
            txs.clone(),
        )?;

        // Extract chain info from block context.
        let chain_info = execution_data.block_context.chain_info();
        let os_chain_info = OsChainInfo {
            chain_id: chain_info.chain_id.clone(),
            strk_fee_token_address: chain_info.fee_token_addresses.strk_fee_token_address,
        };

        // Fetch classes.
        let classes = self.classes_provider.get_classes(&execution_data.executed_class_hashes)?;

        // Fetch storage proofs.
        let storage_proofs =
            self.storage_proofs_provider.get_storage_proofs(block_number, &execution_data)?;

        // Convert execution outputs to CentralTransactionExecutionInfo.
        let tx_execution_infos =
            execution_data.execution_outputs.into_iter().map(|output| output.0.into()).collect();

        // Merge initial_reads with proof_state.
        execution_data.initial_reads.extend(&storage_proofs.proof_state);

        // Assemble OsBlockInput.
        let os_block_input = OsBlockInput {
            // TODO(Aviv): Add block hash commitments.
            block_hash_commitments: BlockHeaderCommitments::default(),
            contract_state_commitment_info: storage_proofs
                .commitment_infos
                .contracts_trie_commitment_info,
            address_to_storage_commitment_info: storage_proofs
                .commitment_infos
                .storage_tries_commitment_infos,
            contract_class_commitment_info: storage_proofs
                .commitment_infos
                .classes_trie_commitment_info,
            transactions: txs
                .into_iter()
                .map(|(invoke_tx, tx_hash)| {
                    starknet_api::executable_transaction::Transaction::Account(
                        starknet_api::executable_transaction::AccountTransaction::Invoke(
                            starknet_api::executable_transaction::InvokeTransaction {
                                tx: invoke_tx,
                                tx_hash,
                            },
                        ),
                    )
                })
                .collect(),
            tx_execution_infos,
            // We assume that no classes are declared.
            declared_class_hash_to_component_hashes: HashMap::new(),
            // We assume that no classes are migrated.
            class_hashes_to_migrate: HashMap::new(),
            // Virtual OS does not update the block hash mapping.
            old_block_number_and_hash: None,
            // Block info and hashes from execution context.
            block_info: execution_data.block_context.block_info().clone(),
            // TODO(Aviv): Add prev and new block hashes.
            prev_block_hash: BlockHash::default(),
            new_block_hash: BlockHash::default(),
            initial_reads: execution_data.initial_reads,
        };

        // 8. Build OsHints.
        Ok(OsHints {
            os_input: StarknetOsInput {
                os_block_inputs: vec![os_block_input],
                // We do not support deprecated contract classes.
                deprecated_compiled_classes: BTreeMap::new(),
                compiled_classes: classes.compiled_classes,
            },
            // TODO(Aviv): choose os hints config
            os_hints_config: OsHintsConfig {
                debug_mode: false,
                full_output: true,
                use_kzg_da: false,
                chain_info: os_chain_info,
                public_keys: None,
                rng_seed_salt: None,
            },
        })
    }

    /// Runs the Starknet OS with the given transactions.
    ///
    /// This method:
    /// 1. Executes transactions to collect state reads
    /// 2. Fetches storage proofs and classes
    /// 3. Builds OS hints
    /// 4. Runs the OS in stateless mode (all state pre-loaded in input)
    ///
    /// Returns the OS output containing the Cairo PIE and execution metrics.
    pub fn run_os(
        &self,
        block_number: BlockNumber,
        contract_class_manager: ContractClassManager,
        txs: Vec<(InvokeTransaction, TransactionHash)>,
    ) -> Result<StarknetOsRunnerOutput, RunnerError> {
        let os_hints = self.create_os_hints(block_number, contract_class_manager, txs)?;
        let output = run_os_stateless(DEFAULT_OS_LAYOUT, os_hints)?;
        Ok(output)
    }
}
