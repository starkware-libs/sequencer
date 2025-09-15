use apollo_gateway_config::compiler_version::VersionId;
use starknet_api::block::GasPrice;
use starknet_api::core::ContractAddress;
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::execution_resources::GasAmount;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::state::SierraContractClass;
use starknet_api::test_utils::declare::{rpc_declare_tx, TEST_SENDER_ADDRESS};
use starknet_api::test_utils::deploy_account::rpc_deploy_account_tx;
use starknet_api::test_utils::invoke::rpc_invoke_tx;
use starknet_api::transaction::fields::{
    AccountDeploymentData,
    AllResourceBounds,
    Calldata,
    PaymasterData,
    ResourceBounds,
    TransactionSignature,
    ValidResourceBounds,
};
use starknet_api::{declare_tx_args, deploy_account_tx_args, felt, invoke_tx_args};
use starknet_types_core::felt::Felt;

pub const NON_EMPTY_RESOURCE_BOUNDS: ResourceBounds =
    ResourceBounds { max_amount: GasAmount(1), max_price_per_unit: GasPrice(1) };

pub fn create_sierra_program(version_id: &VersionId) -> Vec<Felt> {
    let version_id = version_id.0;
    vec![
        // Sierra Version ID.
        Felt::from(u64::try_from(version_id.major).unwrap()),
        Felt::from(u64::try_from(version_id.minor).unwrap()),
        Felt::from(u64::try_from(version_id.patch).unwrap()),
        // Compiler Version ID.
        Felt::from(u64::try_from(0).unwrap()),
        Felt::from(u64::try_from(0).unwrap()),
        Felt::from(u64::try_from(0).unwrap()),
    ]
}

#[derive(Clone)]
pub enum TransactionType {
    Declare,
    DeployAccount,
    Invoke,
}

/// Transaction arguments used for the function [rpc_tx_for_testing].
#[derive(Clone)]
pub struct RpcTransactionArgs {
    pub sender_address: ContractAddress,
    pub resource_bounds: AllResourceBounds,
    pub calldata: Calldata,
    pub signature: TransactionSignature,
    pub account_deployment_data: AccountDeploymentData,
    pub paymaster_data: PaymasterData,
    pub nonce_data_availability_mode: DataAvailabilityMode,
    pub fee_data_availability_mode: DataAvailabilityMode,
}

impl Default for RpcTransactionArgs {
    fn default() -> Self {
        Self {
            sender_address: TEST_SENDER_ADDRESS.into(),
            resource_bounds: AllResourceBounds::default(),
            calldata: Default::default(),
            signature: Default::default(),
            account_deployment_data: Default::default(),
            paymaster_data: Default::default(),
            nonce_data_availability_mode: DataAvailabilityMode::L1,
            fee_data_availability_mode: DataAvailabilityMode::L1,
        }
    }
}

pub fn rpc_tx_for_testing(
    tx_type: TransactionType,
    rpc_tx_args: RpcTransactionArgs,
) -> RpcTransaction {
    let RpcTransactionArgs {
        sender_address,
        resource_bounds,
        calldata,
        signature,
        account_deployment_data,
        paymaster_data,
        nonce_data_availability_mode,
        fee_data_availability_mode,
    } = rpc_tx_args;
    match tx_type {
        TransactionType::Declare => {
            // Minimal contract class.
            let contract_class = SierraContractClass {
                sierra_program: vec![
                    // Sierra Version ID.
                    felt!(1_u32),
                    felt!(3_u32),
                    felt!(0_u32),
                    // Compiler version ID.
                    felt!(1_u32),
                    felt!(3_u32),
                    felt!(0_u32),
                ],
                ..Default::default()
            };
            rpc_declare_tx(
                declare_tx_args!(
                    signature,
                    sender_address,
                    resource_bounds: ValidResourceBounds::AllResources(resource_bounds),
                    account_deployment_data,
                    paymaster_data,
                    nonce_data_availability_mode,
                    fee_data_availability_mode,
                ),
                contract_class,
            )
        }
        TransactionType::DeployAccount => rpc_deploy_account_tx(deploy_account_tx_args!(
            signature,
            resource_bounds: ValidResourceBounds::AllResources(resource_bounds),
            constructor_calldata: calldata,
            paymaster_data,
            nonce_data_availability_mode,
            fee_data_availability_mode,
        )),
        TransactionType::Invoke => rpc_invoke_tx(invoke_tx_args!(
            signature,
            sender_address,
            calldata,
            resource_bounds: ValidResourceBounds::AllResources(resource_bounds),
            account_deployment_data,
            paymaster_data,
            nonce_data_availability_mode,
            fee_data_availability_mode,
        )),
    }
}
