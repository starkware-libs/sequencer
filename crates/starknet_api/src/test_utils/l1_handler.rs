use crate::core::{ContractAddress, EntryPointSelector, Nonce};
use crate::executable_transaction::L1HandlerTransaction as ExecutableL1HandlerTransaction;
use crate::transaction::fields::{Calldata, Fee};
use crate::transaction::{L1HandlerTransaction, TransactionHash, TransactionVersion};

#[derive(Clone)]
pub struct L1HandlerTxArgs {
    pub version: TransactionVersion,
    pub nonce: Nonce,
    pub contract_address: ContractAddress,
    pub entry_point_selector: EntryPointSelector,
    pub calldata: Calldata,
    pub tx_hash: TransactionHash,
    pub paid_fee_on_l1: Fee,
}

impl Default for L1HandlerTxArgs {
    fn default() -> Self {
        L1HandlerTxArgs {
            version: TransactionVersion::THREE,
            nonce: Nonce::default(),
            contract_address: ContractAddress::default(),
            entry_point_selector: EntryPointSelector::default(),
            calldata: Calldata::default(),
            tx_hash: TransactionHash::default(),
            paid_fee_on_l1: Fee::default(),
        }
    }
}

/// Utility macro for creating `L1HandlerTransaction` to reduce boilerplate.
#[macro_export]
macro_rules! l1_handler_tx_args {
    ($($field:ident $(: $value:expr)?),* $(,)?) => {
        $crate::test_utils::l1_handler::L1HandlerTxArgs {
            $($field $(: $value)?,)*
            ..Default::default()
        }
    };
    ($($field:ident $(: $value:expr)?),* , ..$defaults:expr) => {
        $crate::test_utils::l1_handler::L1HandlerTxArgs {
            $($field $(: $value)?,)*
            ..$defaults
        }
    };
}

pub fn executable_l1_handler_tx(
    l1_handler_tx_args: L1HandlerTxArgs,
) -> ExecutableL1HandlerTransaction {
    let tx_version = l1_handler_tx_args.version;
    if tx_version != TransactionVersion::THREE {
        panic!("Unsupported transaction version: {:?}.", l1_handler_tx_args.version);
    }

    ExecutableL1HandlerTransaction {
        tx: L1HandlerTransaction {
            version: tx_version,
            nonce: l1_handler_tx_args.nonce,
            contract_address: l1_handler_tx_args.contract_address,
            entry_point_selector: l1_handler_tx_args.entry_point_selector,
            calldata: l1_handler_tx_args.calldata,
        },
        tx_hash: l1_handler_tx_args.tx_hash,
        paid_fee_on_l1: l1_handler_tx_args.paid_fee_on_l1,
    }
}
