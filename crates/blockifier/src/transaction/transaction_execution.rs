use starknet_api::contract_class::ClassInfo;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::executable_transaction::{
    AccountTransaction as ApiExecutableTransaction,
    DeclareTransaction,
    DeployAccountTransaction,
    InvokeTransaction,
    L1HandlerTransaction,
};
use starknet_api::transaction::fields::Fee;
use starknet_api::transaction::{
    CalculateContractAddress,
    Transaction as StarknetApiTransaction,
    TransactionHash,
};

use crate::bouncer::verify_tx_weights_within_max_capacity;
use crate::context::BlockContext;
use crate::state::cached_state::TransactionalState;
use crate::state::state_api::UpdatableState;
use crate::transaction::account_transaction::{
    AccountTransaction,
    ExecutionFlags as AccountExecutionFlags,
};
use crate::transaction::objects::{
    TransactionExecutionInfo,
    TransactionExecutionResult,
    TransactionInfo,
    TransactionInfoCreator,
};
use crate::transaction::transactions::ExecutableTransaction;

// TODO(Gilad): Move into transaction.rs, makes more sense to be defined there.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, derive_more::From)]
pub enum Transaction {
    Account(AccountTransaction),
    L1Handler(L1HandlerTransaction),
}

impl Transaction {
    pub fn new_for_sequencing(value: starknet_api::executable_transaction::Transaction) -> Self {
        match value {
            starknet_api::executable_transaction::Transaction::Account(tx) => {
                Transaction::Account(AccountTransaction::new_for_sequencing(tx))
            }
            starknet_api::executable_transaction::Transaction::L1Handler(tx) => {
                Transaction::L1Handler(tx)
            }
        }
    }

    pub fn nonce(&self) -> Nonce {
        match self {
            Self::Account(tx) => tx.nonce(),
            Self::L1Handler(tx) => tx.tx.nonce,
        }
    }

    pub fn sender_address(&self) -> ContractAddress {
        match self {
            Self::Account(tx) => tx.sender_address(),
            Self::L1Handler(tx) => tx.tx.contract_address,
        }
    }

    pub fn tx_hash(tx: &Transaction) -> TransactionHash {
        match tx {
            Transaction::Account(tx) => tx.tx_hash(),
            Transaction::L1Handler(tx) => tx.tx_hash,
        }
    }

    pub fn from_api(
        tx: StarknetApiTransaction,
        tx_hash: TransactionHash,
        class_info: Option<ClassInfo>,
        paid_fee_on_l1: Option<Fee>,
        deployed_contract_address: Option<ContractAddress>,
        execution_flags: AccountExecutionFlags,
    ) -> TransactionExecutionResult<Self> {
        let executable_tx = match tx {
            StarknetApiTransaction::L1Handler(l1_handler) => {
                return Ok(Self::L1Handler(L1HandlerTransaction {
                    tx: l1_handler,
                    tx_hash,
                    paid_fee_on_l1: paid_fee_on_l1
                        .expect("L1Handler should be created with the fee paid on L1"),
                }));
            }
            StarknetApiTransaction::Declare(declare) => {
                let non_optional_class_info =
                    class_info.expect("Declare should be created with a ClassInfo.");

                ApiExecutableTransaction::Declare(DeclareTransaction {
                    tx: declare,
                    tx_hash,
                    class_info: non_optional_class_info,
                })
            }
            StarknetApiTransaction::DeployAccount(deploy_account) => {
                let contract_address = match deployed_contract_address {
                    Some(address) => address,
                    None => deploy_account.calculate_contract_address()?,
                };
                ApiExecutableTransaction::DeployAccount(DeployAccountTransaction {
                    tx: deploy_account,
                    tx_hash,
                    contract_address,
                })
            }
            StarknetApiTransaction::Invoke(invoke) => {
                ApiExecutableTransaction::Invoke(InvokeTransaction { tx: invoke, tx_hash })
            }
            _ => unimplemented!(),
        };
        Ok(AccountTransaction { tx: executable_tx, execution_flags }.into())
    }
}

impl TransactionInfoCreator for Transaction {
    fn create_tx_info(&self) -> TransactionInfo {
        match self {
            Self::Account(account_tx) => account_tx.create_tx_info(),
            Self::L1Handler(l1_handler_tx) => l1_handler_tx.create_tx_info(),
        }
    }
}

impl<U: UpdatableState> ExecutableTransaction<U> for Transaction {
    fn execute_raw(
        &self,
        state: &mut TransactionalState<'_, U>,
        block_context: &BlockContext,
        concurrency_mode: bool,
    ) -> TransactionExecutionResult<TransactionExecutionInfo> {
        // TODO(Yoni, 1/8/2024): consider unimplementing the ExecutableTransaction trait for inner
        // types, since now running Transaction::execute_raw is not identical to
        // AccountTransaction::execute_raw.
        let tx_execution_info = match self {
            Self::Account(account_tx) => {
                account_tx.execute_raw(state, block_context, concurrency_mode)?
            }
            Self::L1Handler(tx) => tx.execute_raw(state, block_context, concurrency_mode)?,
        };

        // Check if the transaction is too large to fit any block.
        // TODO(Yoni, 1/8/2024): consider caching these two.
        let tx_execution_summary = tx_execution_info.summarize(&block_context.versioned_constants);
        let tx_builtin_counters = tx_execution_info.summarize_builtins();
        let mut tx_state_changes_keys = state.to_state_diff()?.state_maps.keys();
        tx_state_changes_keys.update_sequencer_key_in_storage(
            &block_context.to_tx_context(self),
            &tx_execution_info,
            concurrency_mode,
        );
        verify_tx_weights_within_max_capacity(
            state,
            &tx_execution_summary,
            &tx_builtin_counters,
            &tx_execution_info.receipt.resources,
            &tx_state_changes_keys,
            &block_context.bouncer_config,
            &block_context.versioned_constants,
        )?;

        Ok(tx_execution_info)
    }
}
