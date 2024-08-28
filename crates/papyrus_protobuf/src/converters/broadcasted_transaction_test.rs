use lazy_static::lazy_static;
use papyrus_test_utils::{get_rng, GetTestInstance};
use starknet_api::rpc_transaction::{
    RpcDeclareTransaction,
    RpcDeclareTransactionV3,
    RpcDeployAccountTransaction,
    RpcDeployAccountTransactionV3,
    RpcInvokeTransaction,
    RpcInvokeTransactionV3,
    RpcTransaction,
};
use starknet_api::transaction::{AllResourceBounds, ResourceBounds};

use crate::mempool::Broadcasted;

#[test]
fn convert_declare_transaction_v3_to_vec_u8_and_back() {
    let mut rng = get_rng();
    let mut rpc_transaction = RpcDeclareTransactionV3::get_test_instance(&mut rng);
    rpc_transaction.resource_bounds = RESOURCE_BOUNDS_MAPPING.clone();
    let rpc_transaction = RpcTransaction::Declare(RpcDeclareTransaction::V3(rpc_transaction));

    convert_transaction_to_vec_u8_and_back(rpc_transaction);
}

#[test]
fn convert_invoke_transaction_v3_to_vec_u8_and_back() {
    let mut rng = get_rng();
    let mut rpc_transaction = RpcInvokeTransactionV3::get_test_instance(&mut rng);

    rpc_transaction.resource_bounds = RESOURCE_BOUNDS_MAPPING.clone();
    let rpc_transaction = RpcTransaction::Invoke(RpcInvokeTransaction::V3(rpc_transaction));

    convert_transaction_to_vec_u8_and_back(rpc_transaction);
}

#[test]
fn convert_deploy_account_transaction_v3_to_vec_u8_and_back() {
    let mut rng = get_rng();
    let mut rpc_transaction = RpcDeployAccountTransactionV3::get_test_instance(&mut rng);
    rpc_transaction.resource_bounds = RESOURCE_BOUNDS_MAPPING.clone();
    let rpc_transaction =
        RpcTransaction::DeployAccount(RpcDeployAccountTransaction::V3(rpc_transaction));

    convert_transaction_to_vec_u8_and_back(rpc_transaction);
}

fn convert_transaction_to_vec_u8_and_back(transaction: RpcTransaction) {
    let data = Broadcasted(Some(transaction.clone()));
    let bytes_data = Vec::<u8>::from(data);
    let res_data = Broadcasted::try_from(bytes_data).unwrap();
    assert_eq!(Broadcasted(Some(transaction)), res_data);
}

lazy_static! {
    static ref RESOURCE_BOUNDS_MAPPING: AllResourceBounds = AllResourceBounds {
        l1_gas: ResourceBounds { max_amount: 0x5, max_price_per_unit: 0x6 },
        l2_gas: ResourceBounds { max_amount: 0x5, max_price_per_unit: 0x6 },
        l1_data_gas: ResourceBounds { max_amount: 0x5, max_price_per_unit: 0x6 },
    };
}
