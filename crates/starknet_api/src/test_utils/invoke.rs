use super::TestingTxArgs;
use crate::abi::abi_utils::selector_from_name;
use crate::calldata;
use crate::core::{ContractAddress, Nonce};
use crate::data_availability::DataAvailabilityMode;
use crate::executable_transaction::{
    AccountTransaction,
    InvokeTransaction as ExecutableInvokeTransaction,
};
use crate::rpc_transaction::{
    InternalRpcTransaction,
    InternalRpcTransactionWithoutTxHash,
    RpcInvokeTransaction,
    RpcInvokeTransactionV3,
    RpcTransaction,
};
use crate::transaction::constants::EXECUTE_ENTRY_POINT_NAME;
use crate::transaction::fields::{
    AccountDeploymentData,
    Calldata,
    Fee,
    PaymasterData,
    Tip,
    TransactionSignature,
    ValidResourceBounds,
};
use crate::transaction::{
    InvokeTransaction,
    InvokeTransactionV0,
    InvokeTransactionV1,
    InvokeTransactionV3,
    TransactionHash,
    TransactionVersion,
};

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
            resource_bounds: ValidResourceBounds::create_for_testing(),
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

pub fn invoke_tx(invoke_args: InvokeTxArgs) -> InvokeTransaction {
    // TODO(Arni): Make TransactionVersion an enum and use match here.
    if invoke_args.version == TransactionVersion::ZERO {
        InvokeTransaction::V0(InvokeTransactionV0 {
            max_fee: invoke_args.max_fee,
            calldata: invoke_args.calldata,
            contract_address: invoke_args.sender_address,
            signature: invoke_args.signature,
            // V0 transactions should always select the `__execute__` entry point.
            entry_point_selector: selector_from_name(EXECUTE_ENTRY_POINT_NAME),
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

pub fn executable_invoke_tx(invoke_args: InvokeTxArgs) -> AccountTransaction {
    let tx_hash = invoke_args.tx_hash;
    let tx = invoke_tx(invoke_args);
    let invoke_tx = ExecutableInvokeTransaction { tx, tx_hash };

    AccountTransaction::Invoke(invoke_tx)
}

pub fn rpc_invoke_tx(invoke_args: InvokeTxArgs) -> RpcTransaction {
    if invoke_args.version != TransactionVersion::THREE {
        panic!("Unsupported transaction version: {:?}.", invoke_args.version);
    }

    let ValidResourceBounds::AllResources(resource_bounds) = invoke_args.resource_bounds else {
        panic!("Unspported resource bounds type: {:?}.", invoke_args.resource_bounds)
    };

    RpcTransaction::Invoke(RpcInvokeTransaction::V3(RpcInvokeTransactionV3 {
        resource_bounds,
        tip: invoke_args.tip,
        calldata: invoke_args.calldata,
        sender_address: invoke_args.sender_address,
        nonce: invoke_args.nonce,
        signature: invoke_args.signature,
        nonce_data_availability_mode: invoke_args.nonce_data_availability_mode,
        fee_data_availability_mode: invoke_args.fee_data_availability_mode,
        paymaster_data: invoke_args.paymaster_data,
        account_deployment_data: invoke_args.account_deployment_data,
    }))
}

pub fn internal_invoke_tx(invoke_args: InvokeTxArgs) -> InternalRpcTransaction {
    if invoke_args.version != TransactionVersion::THREE {
        panic!("Unsupported transaction version: {:?}.", invoke_args.version);
    }
    let tx_hash = invoke_args.tx_hash;
    let RpcTransaction::Invoke(tx) = rpc_invoke_tx(invoke_args) else {
        panic!("Expected RpcTransaction::Invoke");
    };

    let invoke_tx = InternalRpcTransactionWithoutTxHash::Invoke(tx);

    InternalRpcTransaction { tx: invoke_tx, tx_hash }
}

impl TestingTxArgs for InvokeTxArgs {
    fn get_rpc_tx(&self) -> RpcTransaction {
        rpc_invoke_tx(self.clone())
    }

    fn get_internal_tx(&self) -> InternalRpcTransaction {
        internal_invoke_tx(self.clone())
    }
}
