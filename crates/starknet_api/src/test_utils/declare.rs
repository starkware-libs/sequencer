use crate::contract_address;
use crate::contract_class::ClassInfo;
use crate::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use crate::data_availability::DataAvailabilityMode;
use crate::executable_transaction::DeclareTransaction as ExecutableDeclareTransaction;
use crate::rpc_transaction::{
    ContractClass,
    RpcDeclareTransaction,
    RpcDeclareTransactionV3,
    RpcTransaction,
};
use crate::transaction::fields::{
    AccountDeploymentData,
    Fee,
    PaymasterData,
    Tip,
    TransactionSignature,
    ValidResourceBounds,
};
use crate::transaction::{
    DeclareTransaction,
    DeclareTransactionV0V1,
    DeclareTransactionV2,
    DeclareTransactionV3,
    TransactionHash,
    TransactionVersion,
};

pub const TEST_SENDER_ADDRESS: u128 = 0x1000;

#[derive(Clone)]
pub struct DeclareTxArgs {
    pub max_fee: Fee,
    pub signature: TransactionSignature,
    pub sender_address: ContractAddress,
    pub version: TransactionVersion,
    pub resource_bounds: ValidResourceBounds,
    pub tip: Tip,
    pub nonce_data_availability_mode: DataAvailabilityMode,
    pub fee_data_availability_mode: DataAvailabilityMode,
    pub paymaster_data: PaymasterData,
    pub account_deployment_data: AccountDeploymentData,
    pub nonce: Nonce,
    pub class_hash: ClassHash,
    pub compiled_class_hash: CompiledClassHash,
    // TODO(Arni): Consider removing this field.
    pub tx_hash: TransactionHash,
}

impl Default for DeclareTxArgs {
    fn default() -> Self {
        Self {
            max_fee: Fee::default(),
            signature: TransactionSignature::default(),
            sender_address: contract_address!(TEST_SENDER_ADDRESS),
            version: TransactionVersion::THREE,
            resource_bounds: ValidResourceBounds::create_for_testing_no_fee_enforcement(),
            tip: Tip::default(),
            nonce_data_availability_mode: DataAvailabilityMode::L1,
            fee_data_availability_mode: DataAvailabilityMode::L1,
            paymaster_data: PaymasterData::default(),
            account_deployment_data: AccountDeploymentData::default(),
            nonce: Nonce::default(),
            class_hash: ClassHash::default(),
            compiled_class_hash: CompiledClassHash::default(),
            tx_hash: TransactionHash::default(),
        }
    }
}

/// Utility macro for creating `DeclareTxArgs` to reduce boilerplate.
#[macro_export]
macro_rules! declare_tx_args {
    ($($field:ident $(: $value:expr)?),* $(,)?) => {
        $crate::test_utils::declare::DeclareTxArgs {
            $($field $(: $value)?,)*
            ..Default::default()
        }
    };
    ($($field:ident $(: $value:expr)?),* , ..$defaults:expr) => {
        $crate::test_utils::declare::DeclareTxArgs {
            $($field $(: $value)?,)*
            ..$defaults
        }
    };
}

pub fn declare_tx(declare_tx_args: DeclareTxArgs) -> DeclareTransaction {
    // TODO: Make TransactionVersion an enum and use match here.
    if declare_tx_args.version == TransactionVersion::ZERO {
        DeclareTransaction::V0(DeclareTransactionV0V1 {
            max_fee: declare_tx_args.max_fee,
            signature: declare_tx_args.signature,
            sender_address: declare_tx_args.sender_address,
            nonce: declare_tx_args.nonce,
            class_hash: declare_tx_args.class_hash,
        })
    } else if declare_tx_args.version == TransactionVersion::ONE {
        DeclareTransaction::V1(DeclareTransactionV0V1 {
            max_fee: declare_tx_args.max_fee,
            signature: declare_tx_args.signature,
            sender_address: declare_tx_args.sender_address,
            nonce: declare_tx_args.nonce,
            class_hash: declare_tx_args.class_hash,
        })
    } else if declare_tx_args.version == TransactionVersion::TWO {
        DeclareTransaction::V2(DeclareTransactionV2 {
            max_fee: declare_tx_args.max_fee,
            signature: declare_tx_args.signature,
            sender_address: declare_tx_args.sender_address,
            nonce: declare_tx_args.nonce,
            class_hash: declare_tx_args.class_hash,
            compiled_class_hash: declare_tx_args.compiled_class_hash,
        })
    } else if declare_tx_args.version == TransactionVersion::THREE {
        DeclareTransaction::V3(DeclareTransactionV3 {
            signature: declare_tx_args.signature,
            sender_address: declare_tx_args.sender_address,
            resource_bounds: declare_tx_args.resource_bounds,
            tip: declare_tx_args.tip,
            nonce_data_availability_mode: declare_tx_args.nonce_data_availability_mode,
            fee_data_availability_mode: declare_tx_args.fee_data_availability_mode,
            paymaster_data: declare_tx_args.paymaster_data,
            account_deployment_data: declare_tx_args.account_deployment_data,
            nonce: declare_tx_args.nonce,
            class_hash: declare_tx_args.class_hash,
            compiled_class_hash: declare_tx_args.compiled_class_hash,
        })
    } else {
        panic!("Unsupported transaction version: {:?}.", declare_tx_args.version)
    }
}

pub fn executable_declare_tx(
    declare_tx_args: DeclareTxArgs,
    class_info: ClassInfo,
) -> ExecutableDeclareTransaction {
    let tx_hash = declare_tx_args.tx_hash;
    let tx = declare_tx(declare_tx_args);
    ExecutableDeclareTransaction { tx, tx_hash, class_info }
}

pub fn rpc_declare_tx(
    declare_tx_args: DeclareTxArgs,
    contract_class: ContractClass,
) -> RpcTransaction {
    if declare_tx_args.version != TransactionVersion::THREE {
        panic!("Unsupported transaction version: {:?}.", declare_tx_args.version);
    }

    let ValidResourceBounds::AllResources(resource_bounds) = declare_tx_args.resource_bounds else {
        panic!("Unsupported resource bounds type: {:?}.", declare_tx_args.resource_bounds)
    };

    RpcTransaction::Declare(RpcDeclareTransaction::V3(RpcDeclareTransactionV3 {
        contract_class,
        signature: declare_tx_args.signature,
        sender_address: declare_tx_args.sender_address,
        resource_bounds,
        tip: declare_tx_args.tip,
        nonce_data_availability_mode: declare_tx_args.nonce_data_availability_mode,
        fee_data_availability_mode: declare_tx_args.fee_data_availability_mode,
        paymaster_data: declare_tx_args.paymaster_data,
        account_deployment_data: declare_tx_args.account_deployment_data,
        nonce: declare_tx_args.nonce,
        compiled_class_hash: declare_tx_args.compiled_class_hash,
    }))
}
