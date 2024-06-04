use blockifier::test_utils::contracts::FeatureContract;
use blockifier::test_utils::{create_trivial_calldata, CairoVersion, NonceManager};
use serde_json::to_string_pretty;
use starknet_api::calldata;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::external_transaction::{
    ContractClass, ExternalDeclareTransaction, ExternalDeclareTransactionV3,
    ExternalDeployAccountTransaction, ExternalDeployAccountTransactionV3,
    ExternalInvokeTransaction, ExternalInvokeTransactionV3, ExternalTransaction,
    ResourceBoundsMapping,
};
use starknet_api::hash::StarkFelt;
use starknet_api::transaction::{
    AccountDeploymentData, Calldata, ContractAddressSalt, PaymasterData, ResourceBounds, Tip,
    TransactionSignature, TransactionVersion,
};

use crate::{declare_tx_args, deploy_account_tx_args, invoke_tx_args};

pub const VALID_L1_GAS_MAX_AMOUNT: u64 = 2214;
pub const VALID_L1_GAS_MAX_PRICE_PER_UNIT: u128 = 100000000000;

// Utils.
pub enum TransactionType {
    Declare,
    DeployAccount,
    Invoke,
}

pub fn get_sender_address(tx: &ExternalTransaction) -> ContractAddress {
    match tx {
        ExternalTransaction::Declare(ExternalDeclareTransaction::V3(tx)) => tx.sender_address,
        // TODO(Mohammad): Add support for deploy account.
        ExternalTransaction::DeployAccount(ExternalDeployAccountTransaction::V3(_)) => {
            ContractAddress::default()
        }
        ExternalTransaction::Invoke(ExternalInvokeTransaction::V3(tx)) => tx.sender_address,
    }
}

pub fn external_tx_for_testing(
    tx_type: TransactionType,
    resource_bounds: ResourceBoundsMapping,
    calldata: Calldata,
    signature: TransactionSignature,
) -> ExternalTransaction {
    match tx_type {
        TransactionType::Declare => {
            external_declare_tx(declare_tx_args!(resource_bounds, signature))
        }
        TransactionType::DeployAccount => external_deploy_account_tx(
            deploy_account_tx_args!(resource_bounds, constructor_calldata: calldata, signature),
        ),
        TransactionType::Invoke => {
            external_invoke_tx(invoke_tx_args!(signature, resource_bounds, calldata))
        }
    }
}

pub const NON_EMPTY_RESOURCE_BOUNDS: ResourceBounds =
    ResourceBounds { max_amount: 1, max_price_per_unit: 1 };

pub fn create_resource_bounds_mapping(
    l1_resource_bounds: ResourceBounds,
    l2_resource_bounds: ResourceBounds,
) -> ResourceBoundsMapping {
    ResourceBoundsMapping { l1_gas: l1_resource_bounds, l2_gas: l2_resource_bounds }
}

pub fn zero_resource_bounds_mapping() -> ResourceBoundsMapping {
    create_resource_bounds_mapping(ResourceBounds::default(), ResourceBounds::default())
}

pub fn non_zero_resource_bounds_mapping() -> ResourceBoundsMapping {
    ResourceBoundsMapping { l1_gas: NON_EMPTY_RESOURCE_BOUNDS, l2_gas: NON_EMPTY_RESOURCE_BOUNDS }
}

pub fn executable_resource_bounds_mapping() -> ResourceBoundsMapping {
    ResourceBoundsMapping {
        l1_gas: ResourceBounds {
            max_amount: VALID_L1_GAS_MAX_AMOUNT,
            max_price_per_unit: VALID_L1_GAS_MAX_PRICE_PER_UNIT,
        },
        l2_gas: ResourceBounds::default(),
    }
}

pub fn invoke_tx() -> ExternalTransaction {
    let cairo_version = CairoVersion::Cairo1;
    let account_contract = FeatureContract::AccountWithoutValidations(cairo_version);
    let account_address = account_contract.get_instance_address(0);
    let test_contract = FeatureContract::TestContract(cairo_version);
    let test_contract_address = test_contract.get_instance_address(0);
    let calldata = create_trivial_calldata(test_contract_address);
    let mut nonce_manager = NonceManager::default();
    let nonce = nonce_manager.next(account_address);
    external_invoke_tx(invoke_tx_args!(
        signature: TransactionSignature(vec![StarkFelt::ZERO]),
        sender_address: account_address,
        resource_bounds: executable_resource_bounds_mapping(),
        nonce,
        calldata,
    ))
}
// TODO(Ayelet, 28/5/2025): Try unifying the macros.
// TODO(Ayelet, 28/5/2025): Consider moving the macros StarkNet API.
#[macro_export]
macro_rules! invoke_tx_args {
    ($($field:ident $(: $value:expr)?),* $(,)?) => {
        $crate::starknet_api_test_utils::InvokeTxArgs {
            $($field $(: $value)?,)*
            ..Default::default()
        }
    };
    ($($field:ident $(: $value:expr)?),* , ..$defaults:expr) => {
        $crate::starknet_api_test_utils::InvokeTxArgs {
            $($field $(: $value)?,)*
            ..$defaults
        }
    };
}

#[macro_export]
macro_rules! deploy_account_tx_args {
    ($($field:ident $(: $value:expr)?),* $(,)?) => {
        $crate::starknet_api_test_utils::DeployAccountTxArgs {
            $($field $(: $value)?,)*
            ..Default::default()
        }
    };
    ($($field:ident $(: $value:expr)?),* , ..$defaults:expr) => {
        $crate::starknet_api_test_utils::DeployAccountTxArgs {
            $($field $(: $value)?,)*
            ..$defaults
        }
    };
}

#[macro_export]
macro_rules! declare_tx_args {
    ($($field:ident $(: $value:expr)?),* $(,)?) => {
        $crate::starknet_api_test_utils::DeclareTxArgs {
            $($field $(: $value)?,)*
            ..Default::default()
        }
    };
    ($($field:ident $(: $value:expr)?),* , ..$defaults:expr) => {
        $crate::starknet_api_test_utils::DeclareTxArgs {
            $($field $(: $value)?,)*
            ..$defaults
        }
    };
}

#[derive(Clone)]
pub struct InvokeTxArgs {
    pub signature: TransactionSignature,
    pub sender_address: ContractAddress,
    pub calldata: Calldata,
    pub version: TransactionVersion,
    pub resource_bounds: ResourceBoundsMapping,
    pub tip: Tip,
    pub nonce_data_availability_mode: DataAvailabilityMode,
    pub fee_data_availability_mode: DataAvailabilityMode,
    pub paymaster_data: PaymasterData,
    pub account_deployment_data: AccountDeploymentData,
    pub nonce: Nonce,
}

impl Default for InvokeTxArgs {
    fn default() -> Self {
        InvokeTxArgs {
            signature: TransactionSignature::default(),
            sender_address: ContractAddress::default(),
            calldata: calldata![],
            version: TransactionVersion::THREE,
            resource_bounds: zero_resource_bounds_mapping(),
            tip: Tip::default(),
            nonce_data_availability_mode: DataAvailabilityMode::L1,
            fee_data_availability_mode: DataAvailabilityMode::L1,
            paymaster_data: PaymasterData::default(),
            account_deployment_data: AccountDeploymentData::default(),
            nonce: Nonce::default(),
        }
    }
}

#[derive(Clone)]
pub struct DeployAccountTxArgs {
    pub signature: TransactionSignature,
    pub deployer_address: ContractAddress,
    pub version: TransactionVersion,
    pub resource_bounds: ResourceBoundsMapping,
    pub tip: Tip,
    pub nonce_data_availability_mode: DataAvailabilityMode,
    pub fee_data_availability_mode: DataAvailabilityMode,
    pub paymaster_data: PaymasterData,
    pub nonce: Nonce,
    pub class_hash: ClassHash,
    pub contract_address_salt: ContractAddressSalt,
    pub constructor_calldata: Calldata,
}

impl Default for DeployAccountTxArgs {
    fn default() -> Self {
        DeployAccountTxArgs {
            signature: TransactionSignature::default(),
            deployer_address: ContractAddress::default(),
            version: TransactionVersion::THREE,
            resource_bounds: zero_resource_bounds_mapping(),
            tip: Tip::default(),
            nonce_data_availability_mode: DataAvailabilityMode::L1,
            fee_data_availability_mode: DataAvailabilityMode::L1,
            paymaster_data: PaymasterData::default(),
            nonce: Nonce::default(),
            class_hash: ClassHash::default(),
            contract_address_salt: ContractAddressSalt::default(),
            constructor_calldata: Calldata::default(),
        }
    }
}

#[derive(Clone)]
pub struct DeclareTxArgs {
    pub signature: TransactionSignature,
    pub sender_address: ContractAddress,
    pub version: TransactionVersion,
    pub resource_bounds: ResourceBoundsMapping,
    pub tip: Tip,
    pub nonce_data_availability_mode: DataAvailabilityMode,
    pub fee_data_availability_mode: DataAvailabilityMode,
    pub paymaster_data: PaymasterData,
    pub account_deployment_data: AccountDeploymentData,
    pub nonce: Nonce,
    pub class_hash: CompiledClassHash,
    pub contract_class: ContractClass,
}

impl Default for DeclareTxArgs {
    fn default() -> Self {
        Self {
            signature: TransactionSignature::default(),
            sender_address: ContractAddress::default(),
            version: TransactionVersion::THREE,
            resource_bounds: zero_resource_bounds_mapping(),
            tip: Tip::default(),
            nonce_data_availability_mode: DataAvailabilityMode::L1,
            fee_data_availability_mode: DataAvailabilityMode::L1,
            paymaster_data: PaymasterData::default(),
            account_deployment_data: AccountDeploymentData::default(),
            nonce: Nonce::default(),
            class_hash: CompiledClassHash::default(),
            contract_class: ContractClass::default(),
        }
    }
}

pub fn external_invoke_tx(invoke_args: InvokeTxArgs) -> ExternalTransaction {
    match invoke_args.version {
        TransactionVersion::THREE => {
            starknet_api::external_transaction::ExternalTransaction::Invoke(
                starknet_api::external_transaction::ExternalInvokeTransaction::V3(
                    ExternalInvokeTransactionV3 {
                        resource_bounds: invoke_args.resource_bounds,
                        tip: invoke_args.tip,
                        calldata: invoke_args.calldata,
                        sender_address: invoke_args.sender_address,
                        nonce: invoke_args.nonce,
                        signature: invoke_args.signature,
                        nonce_data_availability_mode: invoke_args.nonce_data_availability_mode,
                        fee_data_availability_mode: invoke_args.fee_data_availability_mode,
                        paymaster_data: invoke_args.paymaster_data,
                        account_deployment_data: invoke_args.account_deployment_data,
                    },
                ),
            )
        }
        _ => panic!("Unsupported transaction version: {:?}.", invoke_args.version),
    }
}

pub fn external_deploy_account_tx(deploy_tx_args: DeployAccountTxArgs) -> ExternalTransaction {
    match deploy_tx_args.version {
        TransactionVersion::THREE => {
            starknet_api::external_transaction::ExternalTransaction::DeployAccount(
                starknet_api::external_transaction::ExternalDeployAccountTransaction::V3(
                    ExternalDeployAccountTransactionV3 {
                        resource_bounds: deploy_tx_args.resource_bounds,
                        tip: deploy_tx_args.tip,
                        contract_address_salt: deploy_tx_args.contract_address_salt,
                        class_hash: deploy_tx_args.class_hash,
                        constructor_calldata: deploy_tx_args.constructor_calldata,
                        nonce: deploy_tx_args.nonce,
                        signature: deploy_tx_args.signature,
                        nonce_data_availability_mode: deploy_tx_args.nonce_data_availability_mode,
                        fee_data_availability_mode: deploy_tx_args.fee_data_availability_mode,
                        paymaster_data: deploy_tx_args.paymaster_data,
                    },
                ),
            )
        }
        _ => panic!("Unsupported transaction version: {:?}.", deploy_tx_args.version),
    }
}

pub fn external_declare_tx(declare_tx_args: DeclareTxArgs) -> ExternalTransaction {
    match declare_tx_args.version {
        TransactionVersion::THREE => {
            starknet_api::external_transaction::ExternalTransaction::Declare(
                starknet_api::external_transaction::ExternalDeclareTransaction::V3(
                    ExternalDeclareTransactionV3 {
                        contract_class: declare_tx_args.contract_class,
                        signature: declare_tx_args.signature,
                        sender_address: declare_tx_args.sender_address,
                        resource_bounds: declare_tx_args.resource_bounds,
                        tip: declare_tx_args.tip,
                        nonce_data_availability_mode: declare_tx_args.nonce_data_availability_mode,
                        fee_data_availability_mode: declare_tx_args.fee_data_availability_mode,
                        paymaster_data: declare_tx_args.paymaster_data,
                        account_deployment_data: declare_tx_args.account_deployment_data,
                        nonce: declare_tx_args.nonce,
                        compiled_class_hash: declare_tx_args.class_hash,
                    },
                ),
            )
        }
        _ => panic!("Unsupported transaction version: {:?}.", declare_tx_args.version),
    }
}

pub fn external_tx_to_json(tx: &ExternalTransaction) -> String {
    let mut tx_json = serde_json::to_value(tx)
        .unwrap_or_else(|tx| panic!("Failed to serialize transaction: {tx:?}"));

    // Add type and version manually
    let type_string = match tx {
        ExternalTransaction::Declare(_) => "DECLARE",
        ExternalTransaction::DeployAccount(_) => "DEPLOY_ACCOUNT",
        ExternalTransaction::Invoke(_) => "INVOKE",
    };

    tx_json
        .as_object_mut()
        .unwrap()
        .extend([("type".to_string(), type_string.into()), ("version".to_string(), "0x3".into())]);

    // Serialize back to pretty JSON string
    to_string_pretty(&tx_json).expect("Failed to serialize transaction")
}
