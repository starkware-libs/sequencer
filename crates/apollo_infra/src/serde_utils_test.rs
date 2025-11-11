use std::fmt::Debug;

use apollo_timed_tests::timed_test;
use serde::{Deserialize, Serialize};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::rpc_transaction::{
    RpcDeclareTransaction,
    RpcDeclareTransactionV3,
    RpcDeployAccountTransaction,
    RpcDeployAccountTransactionV3,
    RpcInvokeTransaction,
    RpcInvokeTransactionV3,
    RpcTransaction,
};
use starknet_types_core::felt::Felt;

use crate::serde_utils::SerdeWrapper;

fn test_generic_data_serde<T>(data: T)
where
    T: Serialize + for<'de> Deserialize<'de> + Debug + Clone + PartialEq,
{
    // Serialize and deserialize the data.
    let encoded = SerdeWrapper::new(data.clone()).wrapper_serialize().unwrap();
    let decoded = SerdeWrapper::<T>::wrapper_deserialize(&encoded).unwrap();

    // Assert that the data is the same after serialization and deserialization.
    assert_eq!(data, decoded);
}

#[timed_test]
fn serde_native_type() {
    let data: u32 = 8;
    test_generic_data_serde(data);
}

#[timed_test]
fn serde_struct_type() {
    #[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
    struct TestStruct {
        a: u32,
        b: u32,
    }

    let data: TestStruct = TestStruct { a: 17, b: 8 };
    test_generic_data_serde(data);
}

#[timed_test]
fn serde_felt() {
    let data: Felt = Felt::ONE;
    test_generic_data_serde(data);
}

#[timed_test]
fn serde_rpc_invoke_tx() {
    let invoke_tx = RpcInvokeTransactionV3 {
        sender_address: Default::default(),
        calldata: Default::default(),
        signature: Default::default(),
        nonce: Default::default(),
        resource_bounds: Default::default(),
        tip: Default::default(),
        paymaster_data: Default::default(),
        account_deployment_data: Default::default(),
        nonce_data_availability_mode: DataAvailabilityMode::L1,
        fee_data_availability_mode: DataAvailabilityMode::L1,
    };
    let rpc_invoke_tx = RpcInvokeTransaction::V3(invoke_tx);

    test_generic_data_serde(RpcTransaction::Invoke(rpc_invoke_tx));
}

#[timed_test]
fn serde_rpc_deploy_account_tx() {
    let deploy_account_tx = RpcDeployAccountTransactionV3 {
        signature: Default::default(),
        nonce: Default::default(),
        class_hash: Default::default(),
        resource_bounds: Default::default(),
        contract_address_salt: Default::default(),
        constructor_calldata: Default::default(),
        tip: Default::default(),
        paymaster_data: Default::default(),
        nonce_data_availability_mode: DataAvailabilityMode::L1,
        fee_data_availability_mode: DataAvailabilityMode::L1,
    };
    let rpc_deploy_account_tx = RpcDeployAccountTransaction::V3(deploy_account_tx);

    test_generic_data_serde(RpcTransaction::DeployAccount(rpc_deploy_account_tx));
}

#[timed_test]
fn serde_rpc_declare_tx() {
    let declare_tx = RpcDeclareTransactionV3 {
        sender_address: Default::default(),
        compiled_class_hash: Default::default(),
        signature: Default::default(),
        nonce: Default::default(),
        contract_class: Default::default(),
        resource_bounds: Default::default(),
        tip: Default::default(),
        paymaster_data: Default::default(),
        account_deployment_data: Default::default(),
        nonce_data_availability_mode: DataAvailabilityMode::L1,
        fee_data_availability_mode: DataAvailabilityMode::L1,
    };
    let rpc_declare_tx = RpcDeclareTransaction::V3(declare_tx);

    test_generic_data_serde(RpcTransaction::Declare(rpc_declare_tx));
}
