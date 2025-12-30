use crate::core::{ContractAddress, EntryPointSelector, Nonce};
use crate::executable_transaction::L1HandlerTransaction as ExecutableL1HandlerTransaction;
use crate::transaction::fields::{Calldata, Fee};
use crate::transaction::{L1HandlerTransaction, TransactionHash};

#[derive(Clone, Default)]
pub struct L1HandlerTxArgs {
    pub nonce: Nonce,
    pub contract_address: ContractAddress,
    pub entry_point_selector: EntryPointSelector,
    pub calldata: Calldata,
    pub tx_hash: TransactionHash,
    pub paid_fee_on_l1: Fee,
}

/// Utility macro for creating `L1HandlerTransaction` to reduce boilerplate.
#[macro_export]
macro_rules! l1_handler_tx_args {
    ($($field:ident $(: $value:expr_2021)?),* $(,)?) => {
        $crate::test_utils::l1_handler::L1HandlerTxArgs {
            $($field $(: $value)?,)*
            ..Default::default()
        }
    };
    ($($field:ident $(: $value:expr_2021)?),* , ..$defaults:expr_2021) => {
        $crate::test_utils::l1_handler::L1HandlerTxArgs {
            $($field $(: $value)?,)*
            ..$defaults
        }
    };
}

pub fn executable_l1_handler_tx(
    l1_handler_tx_args: L1HandlerTxArgs,
) -> ExecutableL1HandlerTransaction {
    ExecutableL1HandlerTransaction {
        tx: L1HandlerTransaction {
            version: L1HandlerTransaction::VERSION,
            nonce: l1_handler_tx_args.nonce,
            contract_address: l1_handler_tx_args.contract_address,
            entry_point_selector: l1_handler_tx_args.entry_point_selector,
            calldata: l1_handler_tx_args.calldata,
        },
        tx_hash: l1_handler_tx_args.tx_hash,
        paid_fee_on_l1: l1_handler_tx_args.paid_fee_on_l1,
    }
}
