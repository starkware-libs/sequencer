use apollo_test_utils::{get_rng, GetTestInstance};
use lazy_static::lazy_static;
use rand::random;
use starknet_api::block::GasPrice;
use starknet_api::execution_resources::{Builtin, ExecutionResources, GasAmount, GasVector};
use starknet_api::transaction::fields::{AllResourceBounds, ResourceBounds, ValidResourceBounds};
use starknet_api::transaction::{
    DeclareTransaction,
    DeclareTransactionOutput,
    DeployAccountTransaction,
    DeployAccountTransactionOutput,
    DeployTransactionOutput,
    FullTransaction,
    InvokeTransaction,
    InvokeTransactionOutput,
    L1HandlerTransactionOutput,
    Transaction as StarknetApiTransaction,
    TransactionOutput,
};
use starknet_api::tx_hash;

use crate::sync::DataOrFin;

macro_rules! create_transaction_output {
    ($tx_output_type:ty, $tx_output_enum_variant:ident) => {{
        let mut rng = get_rng();
        let mut transaction_output = <$tx_output_type>::get_test_instance(&mut rng);
        transaction_output.execution_resources = EXECUTION_RESOURCES.clone();
        transaction_output.events = vec![];
        TransactionOutput::$tx_output_enum_variant(transaction_output)
    }};
}

#[test]
fn convert_l1_handler_transaction_to_vec_u8_and_back() {
    let mut rng = get_rng();
    let transaction = starknet_api::transaction::L1HandlerTransaction::get_test_instance(&mut rng);
    let transaction = StarknetApiTransaction::L1Handler(transaction);

    let transaction_output = create_transaction_output!(L1HandlerTransactionOutput, L1Handler);
    assert_transaction_to_vec_u8_and_back(transaction, transaction_output);
}

#[test]
fn convert_deploy_transaction_to_vec_u8_and_back() {
    let mut rng = get_rng();
    let transaction = starknet_api::transaction::DeployTransaction::get_test_instance(&mut rng);
    let transaction = StarknetApiTransaction::Deploy(transaction);

    let transaction_output = create_transaction_output!(DeployTransactionOutput, Deploy);
    assert_transaction_to_vec_u8_and_back(transaction, transaction_output);
}

#[test]
fn convert_declare_transaction_v0_to_vec_u8_and_back() {
    let mut rng = get_rng();
    let transaction =
        starknet_api::transaction::DeclareTransactionV0V1::get_test_instance(&mut rng);
    let transaction = StarknetApiTransaction::Declare(DeclareTransaction::V0(transaction));

    let transaction_output = create_transaction_output!(DeclareTransactionOutput, Declare);
    assert_transaction_to_vec_u8_and_back(transaction, transaction_output);
}

#[test]
fn convert_declare_transaction_v1_to_vec_u8_and_back() {
    let mut rng = get_rng();
    let transaction =
        starknet_api::transaction::DeclareTransactionV0V1::get_test_instance(&mut rng);
    let transaction = StarknetApiTransaction::Declare(DeclareTransaction::V1(transaction));

    let transaction_output = create_transaction_output!(DeclareTransactionOutput, Declare);
    assert_transaction_to_vec_u8_and_back(transaction, transaction_output);
}

#[test]
fn convert_declare_transaction_v2_to_vec_u8_and_back() {
    let mut rng = get_rng();
    let transaction = starknet_api::transaction::DeclareTransactionV2::get_test_instance(&mut rng);
    let transaction = StarknetApiTransaction::Declare(DeclareTransaction::V2(transaction));

    let transaction_output = create_transaction_output!(DeclareTransactionOutput, Declare);
    assert_transaction_to_vec_u8_and_back(transaction, transaction_output);
}

#[test]
fn convert_declare_transaction_v3_to_vec_u8_and_back() {
    let mut rng = get_rng();
    let mut transaction =
        starknet_api::transaction::DeclareTransactionV3::get_test_instance(&mut rng);
    transaction.resource_bounds = *RESOURCE_BOUNDS_MAPPING;
    let transaction = StarknetApiTransaction::Declare(DeclareTransaction::V3(transaction));

    let transaction_output = create_transaction_output!(DeclareTransactionOutput, Declare);
    assert_transaction_to_vec_u8_and_back(transaction, transaction_output);
}

#[test]
fn convert_invoke_transaction_v0_to_vec_u8_and_back() {
    let mut rng = get_rng();
    let transaction = starknet_api::transaction::InvokeTransactionV0::get_test_instance(&mut rng);
    let transaction = StarknetApiTransaction::Invoke(InvokeTransaction::V0(transaction));

    let transaction_output = create_transaction_output!(InvokeTransactionOutput, Invoke);
    assert_transaction_to_vec_u8_and_back(transaction, transaction_output);
}

#[test]
fn convert_invoke_transaction_v1_to_vec_u8_and_back() {
    let mut rng = get_rng();
    let transaction = starknet_api::transaction::InvokeTransactionV1::get_test_instance(&mut rng);
    let transaction = StarknetApiTransaction::Invoke(InvokeTransaction::V1(transaction));

    let transaction_output = create_transaction_output!(InvokeTransactionOutput, Invoke);
    assert_transaction_to_vec_u8_and_back(transaction, transaction_output);
}

#[test]
fn convert_invoke_transaction_v3_to_vec_u8_and_back() {
    let mut rng = get_rng();
    let mut transaction =
        starknet_api::transaction::InvokeTransactionV3::get_test_instance(&mut rng);
    transaction.resource_bounds = *RESOURCE_BOUNDS_MAPPING;
    let transaction = StarknetApiTransaction::Invoke(InvokeTransaction::V3(transaction));

    let transaction_output = create_transaction_output!(InvokeTransactionOutput, Invoke);
    assert_transaction_to_vec_u8_and_back(transaction, transaction_output);
}

#[test]
fn convert_deploy_account_transaction_v1_to_vec_u8_and_back() {
    let mut rng = get_rng();
    let transaction =
        starknet_api::transaction::DeployAccountTransactionV1::get_test_instance(&mut rng);
    let transaction =
        StarknetApiTransaction::DeployAccount(DeployAccountTransaction::V1(transaction));

    let transaction_output =
        create_transaction_output!(DeployAccountTransactionOutput, DeployAccount);
    assert_transaction_to_vec_u8_and_back(transaction, transaction_output);
}

#[test]
fn convert_deploy_account_transaction_v3_to_vec_u8_and_back() {
    let mut rng = get_rng();
    let mut transaction =
        starknet_api::transaction::DeployAccountTransactionV3::get_test_instance(&mut rng);
    transaction.resource_bounds = *RESOURCE_BOUNDS_MAPPING;
    let transaction =
        StarknetApiTransaction::DeployAccount(DeployAccountTransaction::V3(transaction));

    let transaction_output =
        create_transaction_output!(DeployAccountTransactionOutput, DeployAccount);
    assert_transaction_to_vec_u8_and_back(transaction, transaction_output);
}

#[test]
fn fin_transaction_to_bytes_and_back() {
    let bytes_data = Vec::<u8>::from(DataOrFin::<FullTransaction>(None));

    let res_data = DataOrFin::<FullTransaction>::try_from(bytes_data).unwrap();
    assert!(res_data.0.is_none());
}

fn assert_transaction_to_vec_u8_and_back(
    transaction: StarknetApiTransaction,
    transaction_output: TransactionOutput,
) {
    let random_transaction_hash = tx_hash!(random::<u64>());
    let data = DataOrFin(Some(FullTransaction {
        transaction,
        transaction_output,
        transaction_hash: random_transaction_hash,
    }));
    let bytes_data = Vec::<u8>::from(data.clone());
    let res_data = DataOrFin::try_from(bytes_data).unwrap();
    assert_eq!(data, res_data);
}

lazy_static! {
    static ref EXECUTION_RESOURCES: ExecutionResources = ExecutionResources {
        steps: 0,
        builtin_instance_counter: std::collections::HashMap::from([
            (Builtin::RangeCheck, 1),
            (Builtin::Pedersen, 2),
            (Builtin::Poseidon, 3),
            (Builtin::EcOp, 4),
            (Builtin::Ecdsa, 5),
            (Builtin::Bitwise, 6),
            (Builtin::Keccak, 7),
            (Builtin::SegmentArena, 0),
        ]),
        memory_holes: 0,
        da_gas_consumed: GasVector::default(),
        gas_consumed: GasVector::default(),
    };
    static ref RESOURCE_BOUNDS_MAPPING: ValidResourceBounds =
        ValidResourceBounds::AllResources(AllResourceBounds {
            l1_gas: ResourceBounds {
                max_amount: GasAmount(0x5),
                max_price_per_unit: GasPrice(0x6)
            },
            l2_gas: ResourceBounds {
                max_amount: GasAmount(0x500),
                max_price_per_unit: GasPrice(0x600)
            },
            l1_data_gas: ResourceBounds {
                max_amount: GasAmount(0x30),
                max_price_per_unit: GasPrice(0x30)
            }
        });
}

#[test]
fn measure_protobuf_encoding_size_for_5k_calldata() {
    use std::sync::Arc;
    use starknet_api::consensus_transaction::ConsensusTransaction;
    use starknet_api::core::{ContractAddress, Nonce};
    use starknet_api::rpc_transaction::{RpcInvokeTransaction, RpcInvokeTransactionV3, RpcTransaction};
    use starknet_api::transaction::fields::{AllResourceBounds, Calldata, ResourceBounds, Tip, TransactionSignature, AccountDeploymentData, PaymasterData};
    use starknet_api::data_availability::DataAvailabilityMode;
    use starknet_types_core::felt::Felt;
    use prost::Message;
    use crate::protobuf;

    // Create calldata with 5000 elements
    let calldata_size = 5000;
    let calldata_vec: Vec<Felt> = (0..calldata_size).map(|i| Felt::from(i as u64)).collect();
    let calldata = Calldata(Arc::new(calldata_vec));
    
    // Create resource bounds
    let resource_bounds = AllResourceBounds {
        l1_gas: ResourceBounds {
            max_amount: GasAmount(100000),
            max_price_per_unit: GasPrice(1000000000), // 1 gwei
        },
        l2_gas: ResourceBounds {
            max_amount: GasAmount(100000),
            max_price_per_unit: GasPrice(1000000000),
        },
        l1_data_gas: ResourceBounds {
            max_amount: GasAmount(100000),
            max_price_per_unit: GasPrice(1000000000),
        },
    };
    
    // Create an invoke transaction V3
    let invoke_tx = RpcInvokeTransactionV3 {
        resource_bounds,
        tip: Tip(0),
        calldata,
        sender_address: ContractAddress::try_from(Felt::from(0x123456789abcdef_u64)).unwrap(),
        nonce: Nonce(Felt::from(1_u64)),
        signature: TransactionSignature(Arc::new(vec![Felt::from(0x1_u64), Felt::from(0x2_u64)])),
        nonce_data_availability_mode: DataAvailabilityMode::L1,
        fee_data_availability_mode: DataAvailabilityMode::L1,
        paymaster_data: PaymasterData(vec![]),
        account_deployment_data: AccountDeploymentData(vec![]),
    };
    
    // Convert to RPC transaction
    let rpc_tx = RpcTransaction::Invoke(RpcInvokeTransaction::V3(invoke_tx));
    
    // Convert to consensus transaction
    let consensus_tx = ConsensusTransaction::RpcTransaction(rpc_tx);
    
    // Convert to protobuf
    let protobuf_tx: protobuf::ConsensusTransaction = consensus_tx.into();
    
    // Encode to bytes
    let encoded_bytes = protobuf_tx.encode_to_vec();
    
    println!("Invoke transaction with {} calldata elements:", calldata_size);
    println!("Protobuf encoded size: {} bytes", encoded_bytes.len());
    println!("Size in KB: {:.2} KB", encoded_bytes.len() as f64 / 1024.0);
    
    // Let's also break down the size by components
    println!("\nBreakdown:");
    
    // Encode just the calldata part to see its contribution
    let calldata_only = (0..calldata_size).map(|i| Felt::from(i as u64))
        .map(|felt| protobuf::Felt252 { elements: felt.to_bytes_be().to_vec() })
        .collect::<Vec<_>>();
    
    let mut calldata_size_estimate = 0;
    for felt in &calldata_only {
        calldata_size_estimate += felt.encoded_len();
    }
    
    println!("Estimated calldata contribution: {} bytes ({:.1}%)", 
             calldata_size_estimate, 
             (calldata_size_estimate as f64 / encoded_bytes.len() as f64) * 100.0);
    println!("Other transaction fields: {} bytes ({:.1}%)", 
             encoded_bytes.len() - calldata_size_estimate,
             ((encoded_bytes.len() - calldata_size_estimate) as f64 / encoded_bytes.len() as f64) * 100.0);
    
    // Calculate bytes per calldata element
    println!("Average bytes per calldata element: {:.2}", calldata_size_estimate as f64 / calldata_size as f64);
    
    // Also test smaller sizes for comparison
    for size in [100, 1000, 2000, 3000, 4000] {
        let small_calldata = Calldata(Arc::new((0..size).map(|i| Felt::from(i as u64)).collect()));
        let small_invoke_tx = RpcInvokeTransactionV3 {
            resource_bounds: resource_bounds.clone(),
            tip: Tip(0),
            calldata: small_calldata,
            sender_address: ContractAddress::try_from(Felt::from(0x123456789abcdef_u64)).unwrap(),
            nonce: Nonce(Felt::from(1_u64)),
            signature: TransactionSignature(Arc::new(vec![Felt::from(0x1_u64), Felt::from(0x2_u64)])),
            nonce_data_availability_mode: DataAvailabilityMode::L1,
            fee_data_availability_mode: DataAvailabilityMode::L1,
            paymaster_data: PaymasterData(vec![]),
            account_deployment_data: AccountDeploymentData(vec![]),
        };
        
        let small_rpc_tx = RpcTransaction::Invoke(RpcInvokeTransaction::V3(small_invoke_tx));
        let small_consensus_tx = ConsensusTransaction::RpcTransaction(small_rpc_tx);
        let small_protobuf_tx: protobuf::ConsensusTransaction = small_consensus_tx.into();
        let small_encoded_bytes = small_protobuf_tx.encode_to_vec();
        
        println!("Size with {} calldata elements: {} bytes ({:.2} KB)", 
                 size, small_encoded_bytes.len(), small_encoded_bytes.len() as f64 / 1024.0);
    }
}
