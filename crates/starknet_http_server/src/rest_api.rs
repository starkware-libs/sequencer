use axum::extract::State;
use axum::Json;
use hyper::HeaderMap;
use serde::{Deserialize, Serialize};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::rpc_transaction::{
    EntryPointByType,
    RpcDeployAccountTransaction,
    RpcDeployAccountTransactionV3,
    RpcInvokeTransaction,
    RpcInvokeTransactionV3,
    RpcTransaction,
};
use starknet_api::state::SierraContractClass;
use starknet_api::transaction::fields::{
    AccountDeploymentData,
    AllResourceBounds,
    Calldata,
    ContractAddressSalt,
    PaymasterData,
    ResourceBounds,
    Tip,
    TransactionSignature,
};
use starknet_api::transaction::{TransactionHash, TransactionVersion};
use starknet_gateway_types::gateway_types::GatewayInput;
use tracing::{instrument, warn};

use crate::errors::HttpServerError;
use crate::http_server::{
    add_tx_result_as_json,
    record_added_transactions,
    AppState,
    HttpServerResult,
    CLIENT_REGION_HEADER,
};

// TODO: refactor with add_tx.
#[instrument(skip(app_state))]
async fn rest_add_tx(
    State(app_state): State<AppState>,
    headers: HeaderMap,
    tx: RestTransactionV3,
) -> HttpServerResult<Json<TransactionHash>> {
    let gateway_input: GatewayInput = GatewayInput { rpc_tx: tx.into(), message_metadata: None };
    let add_tx_result = app_state.gateway_client.add_tx(gateway_input).await.map_err(|e| {
        warn!("Error while adding transaction: {}", e);
        HttpServerError::from(e)
    });

    let region =
        headers.get(CLIENT_REGION_HEADER).and_then(|region| region.to_str().ok()).unwrap_or("N/A");
    record_added_transactions(&add_tx_result, region);
    add_tx_result_as_json(add_tx_result)
}

#[cfg(test)]
#[path = "rest_api_http_server_test.rs"]
pub mod rest_api_http_server_test;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Hash)]
#[serde(tag = "tx_type")]
#[serde(deny_unknown_fields)]
pub enum RestTransactionV3 {
    #[serde(rename = "DECLARE")]
    Declare(RestDeclareTransactionV3),
    #[serde(rename = "DEPLOY_ACCOUNT")]
    DeployAccount(RestDeployAccountTransactionV3),
    #[serde(rename = "INVOKE_FUNCTION")]
    Invoke(RestInvokeTransactionV3),
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct RestInvokeTransactionV3 {
    pub version: TransactionVersion,
    pub sender_address: ContractAddress,
    pub calldata: Calldata,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub resource_bounds: RestAllResourceBounds,
    pub tip: Tip,
    pub paymaster_data: PaymasterData,
    pub account_deployment_data: AccountDeploymentData,
    pub nonce_data_availability_mode: DataAvailabilityMode,
    pub fee_data_availability_mode: DataAvailabilityMode,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct RestDeployAccountTransactionV3 {
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub class_hash: ClassHash,
    pub contract_address_salt: ContractAddressSalt,
    pub constructor_calldata: Calldata,
    pub resource_bounds: RestAllResourceBounds,
    pub tip: Tip,
    pub paymaster_data: PaymasterData,
    pub nonce_data_availability_mode: DataAvailabilityMode,
    pub fee_data_availability_mode: DataAvailabilityMode,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Hash)]
pub struct RestDeclareTransactionV3 {
    pub sender_address: ContractAddress,
    pub compiled_class_hash: CompiledClassHash,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub contract_class: RestSierraContractClass,
    pub resource_bounds: RestAllResourceBounds,
    pub tip: Tip,
    pub paymaster_data: PaymasterData,
    pub account_deployment_data: AccountDeploymentData,
    pub nonce_data_availability_mode: DataAvailabilityMode,
    pub fee_data_availability_mode: DataAvailabilityMode,
}

#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub struct RestAllResourceBounds {
    pub l1_gas: ResourceBounds,
    pub l2_gas: ResourceBounds,
    pub l1_data_gas: ResourceBounds,
}

impl From<RestAllResourceBounds> for AllResourceBounds {
    fn from(rest_all_resource_bounds: RestAllResourceBounds) -> Self {
        AllResourceBounds {
            l1_gas: rest_all_resource_bounds.l1_gas,
            l2_gas: rest_all_resource_bounds.l2_gas,
            l1_data_gas: rest_all_resource_bounds.l1_data_gas,
        }
    }
}

impl From<AllResourceBounds> for RestAllResourceBounds {
    fn from(all_resource_bounds: AllResourceBounds) -> Self {
        RestAllResourceBounds {
            l1_gas: all_resource_bounds.l1_gas,
            l2_gas: all_resource_bounds.l2_gas,
            l1_data_gas: all_resource_bounds.l1_data_gas,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize, Hash)]
pub struct RestSierraContractClass {
    pub sierra_program: String,
    pub contract_class_version: String,
    pub entry_points_by_type: EntryPointByType,
    pub abi: String,
}

impl From<RestInvokeTransactionV3> for RpcInvokeTransactionV3 {
    fn from(rest_invoke_tx: RestInvokeTransactionV3) -> Self {
        RpcInvokeTransactionV3 {
            sender_address: rest_invoke_tx.sender_address,
            calldata: rest_invoke_tx.calldata,
            signature: rest_invoke_tx.signature,
            nonce: rest_invoke_tx.nonce,
            resource_bounds: rest_invoke_tx.resource_bounds.into(),
            tip: rest_invoke_tx.tip,
            paymaster_data: rest_invoke_tx.paymaster_data,
            account_deployment_data: rest_invoke_tx.account_deployment_data,
            nonce_data_availability_mode: rest_invoke_tx.nonce_data_availability_mode,
            fee_data_availability_mode: rest_invoke_tx.fee_data_availability_mode,
        }
    }
}

impl From<RestDeployAccountTransactionV3> for RpcDeployAccountTransactionV3 {
    fn from(rest_deploy_account_tx: RestDeployAccountTransactionV3) -> Self {
        RpcDeployAccountTransactionV3 {
            signature: rest_deploy_account_tx.signature,
            nonce: rest_deploy_account_tx.nonce,
            class_hash: rest_deploy_account_tx.class_hash,
            contract_address_salt: rest_deploy_account_tx.contract_address_salt,
            constructor_calldata: rest_deploy_account_tx.constructor_calldata,
            resource_bounds: rest_deploy_account_tx.resource_bounds.into(),
            tip: rest_deploy_account_tx.tip,
            paymaster_data: rest_deploy_account_tx.paymaster_data,
            nonce_data_availability_mode: rest_deploy_account_tx.nonce_data_availability_mode,
            fee_data_availability_mode: rest_deploy_account_tx.fee_data_availability_mode,
        }
    }
}

impl From<RestTransactionV3> for RpcTransaction {
    fn from(rest_tx: RestTransactionV3) -> Self {
        match rest_tx {
            RestTransactionV3::Declare(_rest_declare_tx) => {
                unimplemented!()
            }
            RestTransactionV3::DeployAccount(rest_deploy_account_tx) => {
                RpcTransaction::DeployAccount(RpcDeployAccountTransaction::V3(
                    rest_deploy_account_tx.into(),
                ))
            }
            RestTransactionV3::Invoke(rest_invoke_tx) => {
                RpcTransaction::Invoke(RpcInvokeTransaction::V3(rest_invoke_tx.into()))
            }
        }
    }
}

// use resources bounds (see/refactor compilation)

// pub(crate) fn decompress_program(
//     base64_compressed_program: &String,
// ) -> Result<Program, ErrorObjectOwned> {
//     base64::decode(base64_compressed_program).map_err(internal_server_error)?;
//     let compressed_data =
//         base64::decode(base64_compressed_program).map_err(internal_server_error)?;
//     let mut decoder = GzDecoder::new(compressed_data.as_slice());
//     let mut decompressed = Vec::new();
//     decoder.read_to_end(&mut decompressed).map_err(internal_server_error)?;
//     serde_json::from_reader(decompressed.as_slice()).map_err(internal_server_error)
