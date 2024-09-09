use std::sync::LazyLock;

use crate::core::{ContractAddress, EntryPointSelector, Nonce};
use crate::data_availability::DataAvailabilityMode;
use crate::executable_transaction::InvokeTransaction as ExecutableInvokeTransaction;
use crate::transaction::{
    AccountDeploymentData,
    Calldata,
    Fee,
    InvokeTransaction,
    InvokeTransactionV0,
    InvokeTransactionV1,
    InvokeTransactionV3,
    PaymasterData,
    Tip,
    TransactionHash,
    TransactionSignature,
    TransactionVersion,
    ValidResourceBounds,
};
use crate::{calldata, felt};

#[derive(Clone)]
pub struct InvokeTxArgs {
    pub max_fee: Fee,
    pub signature: TransactionSignature,
    pub sender_address: ContractAddress,
    pub calldata: Calldata,
    pub version: TransactionVersion,
    pub resource_bounds: ValidResourceBounds,
    pub tip: Tip,
    pub nonce_data_availability_mode: DataAvailabilityMode,
    pub fee_data_availability_mode: DataAvailabilityMode,
    pub paymaster_data: PaymasterData,
    pub account_deployment_data: AccountDeploymentData,
    pub nonce: Nonce,
    pub only_query: bool,
    pub tx_hash: TransactionHash,
}

impl Default for InvokeTxArgs {
    fn default() -> Self {
        InvokeTxArgs {
            max_fee: Fee::default(),
            signature: TransactionSignature::default(),
            sender_address: ContractAddress::default(),
            calldata: calldata![],
            version: TransactionVersion::THREE,
            resource_bounds: ValidResourceBounds::create_for_testing_no_fee_enforcement(),
            tip: Tip::default(),
            nonce_data_availability_mode: DataAvailabilityMode::L1,
            fee_data_availability_mode: DataAvailabilityMode::L1,
            paymaster_data: PaymasterData::default(),
            account_deployment_data: AccountDeploymentData::default(),
            nonce: Nonce::default(),
            only_query: false,
            tx_hash: TransactionHash::default(),
        }
    }
}

/// Utility macro for creating `InvokeTxArgs` to reduce boilerplate.
#[macro_export]
macro_rules! invoke_tx_args {
    ($($field:ident $(: $value:expr)?),* $(,)?) => {
        $crate::test_utils::invoke::InvokeTxArgs {
            $($field $(: $value)?,)*
            ..Default::default()
        }
    };
    ($($field:ident $(: $value:expr)?),* , ..$defaults:expr) => {
        $crate::test_utils::invoke::InvokeTxArgs {
            $($field $(: $value)?,)*
            ..$defaults
        }
    };
}

/// V0 transactions should always select the `__execute__` entry point.
/// The original code from the blockifier crate was:
///
/// crate::abi::abi_utils::selector_from_name(
///     crate::transaction::constants::EXECUTE_ENTRY_POINT_NAME
/// )
static EXECUTE_ENTRY_POINT_SELECTOR: LazyLock<EntryPointSelector> = LazyLock::new(|| {
    const EXECUTE_ENTRY_POINT_NAME_THROUGH_KECCAK: &str =
        "0x15d40a3d6ca2ac30f4031e42be28da9b056fef9bb7357ac5e85627ee876e5ad";

    EntryPointSelector(felt!(EXECUTE_ENTRY_POINT_NAME_THROUGH_KECCAK))
});

pub fn invoke_tx(invoke_args: InvokeTxArgs) -> InvokeTransaction {
    // TODO: Make TransactionVersion an enum and use match here.
    if invoke_args.version == TransactionVersion::ZERO {
        InvokeTransaction::V0(InvokeTransactionV0 {
            max_fee: invoke_args.max_fee,
            calldata: invoke_args.calldata,
            contract_address: invoke_args.sender_address,
            signature: invoke_args.signature,
            entry_point_selector: *EXECUTE_ENTRY_POINT_SELECTOR,
        })
    } else if invoke_args.version == TransactionVersion::ONE {
        InvokeTransaction::V1(InvokeTransactionV1 {
            max_fee: invoke_args.max_fee,
            sender_address: invoke_args.sender_address,
            nonce: invoke_args.nonce,
            calldata: invoke_args.calldata,
            signature: invoke_args.signature,
        })
    } else if invoke_args.version == TransactionVersion::THREE {
        InvokeTransaction::V3(InvokeTransactionV3 {
            resource_bounds: invoke_args.resource_bounds,
            calldata: invoke_args.calldata,
            sender_address: invoke_args.sender_address,
            nonce: invoke_args.nonce,
            signature: invoke_args.signature,
            tip: invoke_args.tip,
            nonce_data_availability_mode: invoke_args.nonce_data_availability_mode,
            fee_data_availability_mode: invoke_args.fee_data_availability_mode,
            paymaster_data: invoke_args.paymaster_data,
            account_deployment_data: invoke_args.account_deployment_data,
        })
    } else {
        panic!("Unsupported transaction version: {:?}.", invoke_args.version)
    }
}

pub fn executable_invoke_tx(invoke_args: InvokeTxArgs) -> ExecutableInvokeTransaction {
    let tx_hash = invoke_args.tx_hash;
    let tx = invoke_tx(invoke_args);

    ExecutableInvokeTransaction { tx, tx_hash }
}
