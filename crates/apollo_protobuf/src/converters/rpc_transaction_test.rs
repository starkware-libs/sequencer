use apollo_test_utils::{get_rng, GetTestInstance};
use lazy_static::lazy_static;
use rstest::rstest;
use starknet_api::block::GasPrice;
use starknet_api::execution_resources::GasAmount;
use starknet_api::rpc_transaction::{
    RpcDeclareTransaction,
    RpcDeclareTransactionV3,
    RpcDeployAccountTransaction,
    RpcDeployAccountTransactionV3,
    RpcInvokeTransaction,
    RpcInvokeTransactionV3,
    RpcTransaction,
};
use starknet_api::transaction::fields::{AllResourceBounds, Proof, ProofFacts, ResourceBounds};

use crate::mempool::RpcTransactionBatch;

#[test]
fn convert_declare_transaction_v3_to_vec_u8_and_back() {
    let mut rng = get_rng();
    let mut rpc_transaction = RpcDeclareTransactionV3::get_test_instance(&mut rng);
    rpc_transaction.resource_bounds = *RESOURCE_BOUNDS_MAPPING;
    let rpc_transaction = RpcTransaction::Declare(RpcDeclareTransaction::V3(rpc_transaction));

    assert_transaction_to_vec_u8_and_back(rpc_transaction);
}

#[rstest]
#[case::without_client_side_proving(false)]
#[case::with_client_side_proving(true)]
fn convert_invoke_transaction_v3_to_vec_u8_and_back(#[case] with_client_side_proving: bool) {
    let mut rng = get_rng();
    let mut rpc_transaction = RpcInvokeTransactionV3::get_test_instance(&mut rng);

    rpc_transaction.resource_bounds = *RESOURCE_BOUNDS_MAPPING;

    if with_client_side_proving {
        rpc_transaction.proof = Proof::proof_for_testing();
        rpc_transaction.proof_facts = ProofFacts::snos_proof_facts_for_testing();
    }

    let rpc_transaction = RpcTransaction::Invoke(RpcInvokeTransaction::V3(rpc_transaction));

    assert_transaction_to_vec_u8_and_back(rpc_transaction);
}

#[test]
fn convert_deploy_account_transaction_v3_to_vec_u8_and_back() {
    let mut rng = get_rng();
    let mut rpc_transaction = RpcDeployAccountTransactionV3::get_test_instance(&mut rng);
    rpc_transaction.resource_bounds = *RESOURCE_BOUNDS_MAPPING;
    let rpc_transaction =
        RpcTransaction::DeployAccount(RpcDeployAccountTransaction::V3(rpc_transaction));

    assert_transaction_to_vec_u8_and_back(rpc_transaction);
}

/// Verifies lossless protobuf serialization/deserialization for mempool p2p propagation.
/// Used by MempoolP2pPropagator for broadcasting new transactions to peers.
fn assert_transaction_to_vec_u8_and_back(transaction: RpcTransaction) {
    let data = RpcTransactionBatch(vec![transaction.clone()]);

    // Serialize: RpcTransactionBatch → protobuf::MempoolTransactionBatch → bytes.
    let bytes_data = Vec::<u8>::from(data);

    // Deserialize: bytes → protobuf::MempoolTransactionBatch → RpcTransactionBatch.
    let res_data = RpcTransactionBatch::try_from(bytes_data).unwrap();

    // Verify round-trip is lossless.
    assert_eq!(RpcTransactionBatch(vec![transaction]), res_data);
}

lazy_static! {
    static ref RESOURCE_BOUNDS_MAPPING: AllResourceBounds = AllResourceBounds {
        l1_gas: ResourceBounds { max_amount: GasAmount(0x5), max_price_per_unit: GasPrice(0x6) },
        l2_gas: ResourceBounds { max_amount: GasAmount(0x6), max_price_per_unit: GasPrice(0x7) },
        l1_data_gas: ResourceBounds {
            max_amount: GasAmount(0x7),
            max_price_per_unit: GasPrice(0x8)
        },
    };
}
