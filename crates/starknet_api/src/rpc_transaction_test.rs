use rstest::rstest;
use starknet_types_core::felt::Felt;

use crate::block::GasPrice;
use crate::core::CompiledClassHash;
use crate::execution_resources::GasAmount;
use crate::rpc_transaction::{
    ContractClass,
    DataAvailabilityMode,
    RpcDeclareTransaction,
    RpcDeclareTransactionV3,
    RpcTransaction,
};
use crate::test_utils::deploy_account::{rpc_deploy_account_tx, DeployAccountTxArgs};
use crate::test_utils::invoke::{rpc_invoke_tx, InvokeTxArgs};
use crate::transaction::fields::{
    AccountDeploymentData,
    AllResourceBounds,
    ContractAddressSalt,
    PaymasterData,
    ResourceBounds,
    Tip,
    TransactionSignature,
    ValidResourceBounds,
};
use crate::{calldata, class_hash, contract_address, felt, nonce};

// TODO: Delete this when starknet_api_test_util is moved to StarkNet API.
fn create_resource_bounds_for_testing() -> AllResourceBounds {
    AllResourceBounds {
        l1_gas: ResourceBounds { max_amount: GasAmount(100), max_price_per_unit: GasPrice(12) },
        l2_gas: ResourceBounds { max_amount: GasAmount(58), max_price_per_unit: GasPrice(31) },
        l1_data_gas: ResourceBounds { max_amount: GasAmount(66), max_price_per_unit: GasPrice(25) },
    }
}

fn create_declare_v3() -> RpcDeclareTransaction {
    RpcDeclareTransaction::V3(RpcDeclareTransactionV3 {
        contract_class: ContractClass::default(),
        resource_bounds: create_resource_bounds_for_testing(),
        tip: Tip(1),
        signature: TransactionSignature(vec![Felt::ONE, Felt::TWO]),
        nonce: nonce!(1),
        compiled_class_hash: CompiledClassHash(Felt::TWO),
        sender_address: contract_address!("0x3"),
        nonce_data_availability_mode: DataAvailabilityMode::L1,
        fee_data_availability_mode: DataAvailabilityMode::L2,
        paymaster_data: PaymasterData(vec![Felt::ZERO]),
        account_deployment_data: AccountDeploymentData(vec![Felt::THREE]),
    })
}

fn create_deploy_account() -> RpcTransaction {
    rpc_deploy_account_tx(DeployAccountTxArgs {
        resource_bounds: ValidResourceBounds::AllResources(create_resource_bounds_for_testing()),
        contract_address_salt: ContractAddressSalt(felt!("0x1")),
        class_hash: class_hash!("0x2"),
        constructor_calldata: calldata![felt!("0x1"), felt!("0x2")],
        nonce: nonce!(1),
        signature: TransactionSignature(vec![felt!("0x1")]),
        nonce_data_availability_mode: DataAvailabilityMode::L2,
        paymaster_data: PaymasterData(vec![felt!("0x2"), felt!("0x0")]),
        ..Default::default()
    })
}

fn create_rpc_invoke_tx() -> RpcTransaction {
    rpc_invoke_tx(InvokeTxArgs {
        resource_bounds: ValidResourceBounds::AllResources(create_resource_bounds_for_testing()),
        calldata: calldata![felt!("0x1"), felt!("0x2")],
        sender_address: contract_address!("0x1"),
        nonce: nonce!(1),
        paymaster_data: PaymasterData(vec![Felt::TWO, Felt::ZERO]),
        account_deployment_data: AccountDeploymentData(vec![felt!("0x1")]),
        ..Default::default()
    })
}

// Test the custom serde/deserde of RPC transactions.
#[rstest]
#[case(RpcTransaction::Declare(create_declare_v3()))]
#[case(create_deploy_account())]
#[case(create_rpc_invoke_tx())]
fn test_rpc_transactions(#[case] tx: RpcTransaction) {
    let serialized = serde_json::to_string(&tx).unwrap();
    let deserialized: RpcTransaction = serde_json::from_str(&serialized).unwrap();
    assert_eq!(tx, deserialized);
}
