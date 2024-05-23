use blockifier::test_utils::contracts::FeatureContract;
use blockifier::test_utils::{create_trivial_calldata, CairoVersion, NonceManager};
use serde_json::to_string_pretty;
use starknet_api::calldata;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::external_transaction::{
    ExternalDeclareTransaction, ExternalDeclareTransactionV3, ExternalDeployAccountTransaction,
    ExternalDeployAccountTransactionV3, ExternalInvokeTransaction, ExternalInvokeTransactionV3,
    ExternalTransaction,
};
use starknet_api::hash::StarkFelt;
use starknet_api::internal_transaction::{InternalInvokeTransaction, InternalTransaction};
use starknet_api::transaction::{
    AccountDeploymentData, Calldata, InvokeTransaction, InvokeTransactionV3, PaymasterData,
    ResourceBounds, ResourceBoundsMapping, Tip, TransactionSignature, TransactionVersion,
};

use crate::invoke_tx_args;

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

pub fn create_internal_tx_for_testing() -> InternalTransaction {
    let tx = InvokeTransactionV3 {
        resource_bounds: ResourceBoundsMapping::try_from(vec![
            (starknet_api::transaction::Resource::L1Gas, ResourceBounds::default()),
            (starknet_api::transaction::Resource::L2Gas, ResourceBounds::default()),
        ])
        .expect("Resource bounds mapping has unexpected structure."),
        signature: Default::default(),
        nonce: Default::default(),
        sender_address: Default::default(),
        calldata: Default::default(),
        nonce_data_availability_mode: DataAvailabilityMode::L1,
        fee_data_availability_mode: DataAvailabilityMode::L1,
        paymaster_data: Default::default(),
        account_deployment_data: Default::default(),
        tip: Default::default(),
    };

    InternalTransaction::Invoke(InternalInvokeTransaction {
        tx: InvokeTransaction::V3(tx),
        tx_hash: Default::default(),
        only_query: false,
    })
}

pub fn external_tx_for_testing(
    transaction_type: TransactionType,
    resource_bounds: ResourceBoundsMapping,
    calldata: Calldata,
    signature: TransactionSignature,
) -> ExternalTransaction {
    match transaction_type {
        TransactionType::Declare => external_declare_tx_for_testing(resource_bounds, signature),
        TransactionType::DeployAccount => {
            external_deploy_account_tx_for_testing(resource_bounds, calldata, signature)
        }
        TransactionType::Invoke => {
            external_invoke_tx(invoke_tx_args!(signature, resource_bounds, calldata))
        }
    }
}

fn external_declare_tx_for_testing(
    resource_bounds: ResourceBoundsMapping,
    signature: TransactionSignature,
) -> ExternalTransaction {
    ExternalTransaction::Declare(ExternalDeclareTransaction::V3(ExternalDeclareTransactionV3 {
        resource_bounds,
        contract_class: Default::default(),
        tip: Default::default(),
        signature,
        nonce: Default::default(),
        compiled_class_hash: Default::default(),
        sender_address: Default::default(),
        nonce_data_availability_mode: DataAvailabilityMode::L1,
        fee_data_availability_mode: DataAvailabilityMode::L1,
        paymaster_data: Default::default(),
        account_deployment_data: Default::default(),
    }))
}

fn external_deploy_account_tx_for_testing(
    resource_bounds: ResourceBoundsMapping,
    constructor_calldata: Calldata,
    signature: TransactionSignature,
) -> ExternalTransaction {
    ExternalTransaction::DeployAccount(ExternalDeployAccountTransaction::V3(
        ExternalDeployAccountTransactionV3 {
            resource_bounds,
            tip: Default::default(),
            contract_address_salt: Default::default(),
            class_hash: Default::default(),
            constructor_calldata,
            nonce: Default::default(),
            signature,
            nonce_data_availability_mode: DataAvailabilityMode::L1,
            fee_data_availability_mode: DataAvailabilityMode::L1,
            paymaster_data: Default::default(),
        },
    ))
}

pub const NON_EMPTY_RESOURCE_BOUNDS: ResourceBounds =
    ResourceBounds { max_amount: 1, max_price_per_unit: 1 };

pub fn create_resource_bounds_mapping(
    l1_resource_bounds: ResourceBounds,
    l2_resource_bounds: ResourceBounds,
) -> ResourceBoundsMapping {
    ResourceBoundsMapping::try_from(vec![
        (starknet_api::transaction::Resource::L1Gas, l1_resource_bounds),
        (starknet_api::transaction::Resource::L2Gas, l2_resource_bounds),
    ])
    .expect("Resource bounds mapping has unexpected structure.")
}

pub fn zero_resource_bounds_mapping() -> ResourceBoundsMapping {
    create_resource_bounds_mapping(ResourceBounds::default(), ResourceBounds::default())
}

pub fn non_zero_resource_bounds_mapping() -> ResourceBoundsMapping {
    create_resource_bounds_mapping(NON_EMPTY_RESOURCE_BOUNDS, NON_EMPTY_RESOURCE_BOUNDS)
}

pub fn executable_resource_bounds_mapping() -> ResourceBoundsMapping {
    ResourceBoundsMapping::try_from(vec![
        (
            starknet_api::transaction::Resource::L1Gas,
            ResourceBounds {
                max_amount: VALID_L1_GAS_MAX_AMOUNT,
                max_price_per_unit: VALID_L1_GAS_MAX_PRICE_PER_UNIT,
            },
        ),
        (starknet_api::transaction::Resource::L2Gas, ResourceBounds::default()),
    ])
    .expect("Resource bounds mapping has unexpected structure.")
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
    external_invoke_tx(invoke_tx_args! {
     signature: TransactionSignature(vec![StarkFelt::ZERO]),
     contract_address: account_address,
     resource_bounds: executable_resource_bounds_mapping(),
     nonce,
     calldata
    })
}

#[derive(Clone)]
pub struct InvokeTxArgs {
    pub signature: TransactionSignature,
    pub contract_address: ContractAddress,
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
            contract_address: ContractAddress::default(),
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

// TODO(Ayelet, 15/5/2025): Consider moving this to StarkNet API.
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

pub fn external_invoke_tx(invoke_args: InvokeTxArgs) -> ExternalTransaction {
    match invoke_args.version {
        TransactionVersion::THREE => {
            starknet_api::external_transaction::ExternalTransaction::Invoke(
                starknet_api::external_transaction::ExternalInvokeTransaction::V3(
                    ExternalInvokeTransactionV3 {
                        resource_bounds: invoke_args.resource_bounds,
                        tip: invoke_args.tip,
                        calldata: invoke_args.calldata,
                        sender_address: invoke_args.contract_address,
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

pub fn external_invoke_tx_to_json(tx: ExternalTransaction) -> String {
    // Serialize to JSON
    let mut tx_json = serde_json::to_value(&tx)
        .unwrap_or_else(|_| panic!("Failed to serialize transaction: {tx:?}"));

    // Add type and version manually
    tx_json.as_object_mut().unwrap().extend([
        ("type".to_string(), "INVOKE_FUNCTION".into()),
        ("version".to_string(), "0x3".into()),
    ]);

    // Serialize back to pretty JSON string
    to_string_pretty(&tx_json).expect("Failed to serialize transaction")
}
