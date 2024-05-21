use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::external_transaction::{
    ExternalDeclareTransaction,
    ExternalDeclareTransactionV3,
    ExternalDeployAccountTransaction,
    ExternalDeployAccountTransactionV3,
    ExternalInvokeTransaction,
    ExternalInvokeTransactionV3,
    ExternalTransaction,
};
use starknet_api::transaction::{
    Calldata,
    ResourceBounds,
    ResourceBoundsMapping,
    TransactionSignature,
};

pub const VALID_L1_GAS_MAX_AMOUNT: u64 = 1662;
pub const VALID_L1_GAS_MAX_PRICE_PER_UNIT: u128 = 100000000000;

// Utils.
pub enum TransactionType {
    Declare,
    DeployAccount,
    Invoke,
}

pub fn external_tx_for_testing(
    transaction_type: TransactionType,
    resource_bounds: ResourceBoundsMapping,
    calldata: Option<Calldata>,
    signature: TransactionSignature,
) -> ExternalTransaction {
    match transaction_type {
        TransactionType::Declare => external_declare_tx_for_testing(resource_bounds, signature),
        TransactionType::DeployAccount => external_deploy_account_tx_for_testing(
            resource_bounds,
            calldata.expect("Calldata is missing."),
            signature,
        ),
        TransactionType::Invoke => external_invoke_tx_for_testing(
            resource_bounds,
            calldata.expect("Calldata is missing."),
            signature,
        ),
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

fn external_invoke_tx_for_testing(
    resource_bounds: ResourceBoundsMapping,
    calldata: Calldata,
    signature: TransactionSignature,
) -> ExternalTransaction {
    executable_external_invoke_tx_for_testing(
        resource_bounds,
        Nonce::default(),
        ContractAddress::default(),
        calldata,
        signature,
    )
}

// TODO(yael 24/4/24): remove this function and generalize the external_tx_for_testing function.
// and add a struct for default args (ExteranlTransactionForTestingArgs)
pub fn executable_external_invoke_tx_for_testing(
    resource_bounds: ResourceBoundsMapping,
    nonce: Nonce,
    sender_address: ContractAddress,
    calldata: Calldata,
    signature: TransactionSignature,
) -> ExternalTransaction {
    ExternalTransaction::Invoke(ExternalInvokeTransaction::V3(ExternalInvokeTransactionV3 {
        resource_bounds,
        tip: Default::default(),
        signature,
        nonce,
        sender_address,
        calldata,
        nonce_data_availability_mode: DataAvailabilityMode::L1,
        fee_data_availability_mode: DataAvailabilityMode::L1,
        paymaster_data: Default::default(),
        account_deployment_data: Default::default(),
    }))
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
