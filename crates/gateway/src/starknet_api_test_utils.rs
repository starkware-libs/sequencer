use blockifier::test_utils::contracts::FeatureContract;
use blockifier::test_utils::{create_trivial_calldata, CairoVersion, NonceManager};
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
    Calldata, InvokeTransaction, InvokeTransactionV3, ResourceBounds, ResourceBoundsMapping,
    TransactionSignature,
};

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

pub fn invoke_tx() -> ExternalTransaction {
    let cairo_version = CairoVersion::Cairo1;
    let account_contract = FeatureContract::AccountWithoutValidations(cairo_version);
    let account_address = account_contract.get_instance_address(0);
    let test_contract = FeatureContract::TestContract(cairo_version);
    let test_contract_address = test_contract.get_instance_address(0);
    let calldata = create_trivial_calldata(test_contract_address);
    let mut nonce_manager = NonceManager::default();
    let nonce = nonce_manager.next(account_address);
    executable_external_invoke_tx_for_testing(
        executable_resource_bounds_mapping(),
        nonce,
        account_address,
        calldata,
        TransactionSignature(vec![StarkFelt::ZERO]),
    )
}
