use std::convert::From;
use std::net::{IpAddr, Ipv4Addr};

use blockifier_test_utils::cairo_versions::CairoVersion;
use mempool_test_utils::starknet_api_test_utils::invoke_tx;
use starknet_api::rpc_transaction::{RpcInvokeTransaction, RpcInvokeTransactionV3, RpcTransaction};
use starknet_api::test_utils::read_json_file;
use starknet_api::transaction::{TransactionHash, TransactionVersion};
use starknet_gateway_types::communication::MockGatewayClient;
use starknet_infra_utils::test_utils::{AvailablePorts, TestIdentifier};
use starknet_types_core::felt::Felt;

use super::{
    RestAllResourceBounds,
    RestDeclareTransactionV3,
    RestDeployAccountTransactionV3,
    RestInvokeTransactionV3,
};
use crate::config::HttpServerConfig;
use crate::test_utils::http_client_server_setup;

#[tokio::test]
async fn deserialization() {
    rest_invoke_tx();
    rest_deploy_account_tx();
    rest_declare_tx();
}

fn rest_invoke_tx() -> RestInvokeTransactionV3 {
    serde_json::from_value(read_json_file("rest_api/invoke_tx.json"))
        .expect("Failed to deserialize RestInvokeTransactionV3")
}

fn rest_deploy_account_tx() -> RestDeployAccountTransactionV3 {
    serde_json::from_value(read_json_file("rest_api/deploy_account_tx.json"))
        .expect("Failed to deserialize DeployAccountTransactionV3")
}

fn rest_declare_tx() -> RestDeclareTransactionV3 {
    serde_json::from_value(read_json_file("rest_api/declare_tx.json"))
        .expect("Failed to deserialize RestDeclareTransactionV3")
}
impl From<RpcInvokeTransactionV3> for RestInvokeTransactionV3 {
    fn from(value: RpcInvokeTransactionV3) -> Self {
        Self {
            version: TransactionVersion::THREE,
            calldata: value.calldata,
            tip: value.tip,
            resource_bounds: RestAllResourceBounds::from(value.resource_bounds),
            paymaster_data: value.paymaster_data,
            sender_address: value.sender_address,
            signature: value.signature,
            nonce: value.nonce,
            account_deployment_data: value.account_deployment_data,
            nonce_data_availability_mode: value.nonce_data_availability_mode,
            fee_data_availability_mode: value.fee_data_availability_mode,
        }
    }
}

#[tokio::test]
async fn test_response() {
    let mut mock_gateway_client = MockGatewayClient::new();

    // Set the successful response.
    let expected_tx_hash = TransactionHash(Felt::ONE);
    mock_gateway_client.expect_add_tx().times(1).return_const(Ok(expected_tx_hash));

    let ip = IpAddr::from(Ipv4Addr::LOCALHOST);
    let mut available_ports = AvailablePorts::new(TestIdentifier::HttpServerUnitTests.into(), 3);
    let http_server_config = HttpServerConfig { ip, port: available_ports.get_next_port() };
    let _add_tx_http_client =
        http_client_server_setup(mock_gateway_client, http_server_config).await;

    // Test a successful response.
    let rpc_tx = invoke_tx(CairoVersion::default());
    let rpc_invoke_tx = match rpc_tx {
        RpcTransaction::Invoke(RpcInvokeTransaction::V3(tx)) => tx,
        _ => panic!("Expected RpcInvokeTransaction::V3"),
    };
    let _rest_invoke_tx = RestInvokeTransactionV3::from(rpc_invoke_tx);

    // let tx_hash = add_tx_http_client.assert_add_tx_success(rpc_tx).await;
    // assert_eq!(tx_hash, expected_tx_hash);
}
