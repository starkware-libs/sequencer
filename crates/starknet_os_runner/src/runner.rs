use std::collections::HashMap;

use blockifier::state::contract_class_manager::ContractClassManager;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::transaction::{InvokeTransaction, TransactionHash};
use starknet_os::io::os_input::{OsBlockInput, StarknetOsInput};

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

    /// Creates OS input from execution data and storage proofs.
    pub fn create_os_input(
        &self,
        block_number: BlockNumber,
        contract_class_manager: ContractClassManager,
        txs: Vec<(InvokeTransaction, TransactionHash)>,
    ) -> Result<StarknetOsInput, RunnerError> {
        // 1. Execute virtual block and get execution data.
        let mut execution_data = self.virtual_block_executor.execute(
            block_number,
            contract_class_manager,
            txs.clone(),
        )?;

        // 2. Fetch classes (consuming executed_class_hashes).
        let classes = self.classes_provider.get_classes(&execution_data.executed_class_hashes)?;

        // 3. Fetch storage proofs (pass execution_data by reference to avoid moving yet).
        let storage_proofs =
            self.storage_proofs_provider.get_storage_proofs(block_number, &execution_data)?;

        // 4. Convert execution outputs to CentralTransactionExecutionInfo (consuming).
        let tx_execution_infos =
            execution_data.execution_outputs.into_iter().map(|output| output.0.into()).collect();

        // 5. Merge initial_reads with proof_state.
        execution_data.initial_reads.extend(&storage_proofs.proof_state);

        // 6. Assemble OsBlockInput.
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
            // Stored block hash buffer skipped for now.
            old_block_number_and_hash: None,
            // Block info and hashes from execution context.
            block_info: execution_data.block_context.block_info().clone(),
            // TODO(Aviv): Add prev and new block hashes.
            prev_block_hash: BlockHash::default(),
            new_block_hash: BlockHash::default(),
            initial_reads: execution_data.initial_reads,
        };

        // 7. Final StarknetOsInput (consuming classes).
        Ok(StarknetOsInput {
            os_block_inputs: vec![os_block_input],
            deprecated_compiled_classes: classes.deprecated_compiled_classes,
            compiled_classes: classes.compiled_classes,
        })
    }
}
