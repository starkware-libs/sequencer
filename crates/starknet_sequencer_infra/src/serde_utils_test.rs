use std::fmt::Debug;

use serde::{Deserialize, Serialize};
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::data_availability::{DataAvailabilityMode};
use starknet_api::rpc_transaction::RpcInvokeTransactionV3;
use starknet_api::transaction::fields::{
    AccountDeploymentData,
    AllResourceBounds,
    Calldata,
    PaymasterData,
    Tip,
    TransactionSignature,
};
use starknet_types_core::felt::Felt;

use crate::serde_utils::BincodeSerdeWrapper;

fn test_generic_data_serde<T>(data: T)
where
    T: Serialize + for<'de> Deserialize<'de> + Debug + Clone + PartialEq,
{
    // Serialize and deserialize the data.
    let encoded = BincodeSerdeWrapper::new(data.clone()).to_bincode().unwrap();
    let decoded = BincodeSerdeWrapper::<T>::from_bincode(&encoded).unwrap();

    // Assert that the data is the same after serialization and deserialization.
    assert_eq!(data, decoded);
}

#[test]
fn test_serde_native_type() {
    let data: u32 = 8;
    test_generic_data_serde(data);
}

#[test]
fn test_serde_struct_type() {
    #[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
    struct TestStruct {
        a: u32,
        b: u32,
    }

    let data: TestStruct = TestStruct { a: 17, b: 8 };
    test_generic_data_serde(data);
}

#[test]
fn test_serde_felt() {
    let data: Felt = Felt::ONE;
    test_generic_data_serde(data);
}

#[test]
fn test_serde_rpc_invoke_tx() {
    let tx = RpcInvokeTransactionV3 {
        sender_address: ContractAddress::default(),
        calldata: Calldata::default(),
        signature: TransactionSignature::default(),
        nonce: Nonce::default(),
        resource_bounds: AllResourceBounds::default(),
        tip: Tip::default(),
        paymaster_data: PaymasterData::default(),
        account_deployment_data: AccountDeploymentData::default(),
        nonce_data_availability_mode: DataAvailabilityMode::L1,
        fee_data_availability_mode: DataAvailabilityMode::L1,
    };
    test_generic_data_serde(tx);
}


#[test]
fn test_serde_sender_tx_fields() {
    test_generic_data_serde(DataAvailabilityMode::L1);
}
