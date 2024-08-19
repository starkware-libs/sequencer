#[cfg(test)]
#[path = "rpc_transaction_test.rs"]
mod rpc_transaction_test;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use starknet_types_core::felt::Felt;

use crate::core::{
    calculate_contract_address,
    ClassHash,
    CompiledClassHash,
    ContractAddress,
    Nonce,
};
use crate::data_availability::DataAvailabilityMode;
use crate::state::EntryPoint;
use crate::transaction::{
    AccountDeploymentData,
    Calldata,
    ContractAddressSalt,
    PaymasterData,
    Resource,
    ResourceBounds,
    Tip,
    TransactionSignature,
};
use crate::StarknetApiError;

/// Transactions that are ready to be broadcasted to the network through RPC and are not included in
/// a block.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type")]
#[serde(deny_unknown_fields)]
pub enum RpcTransaction {
    #[serde(rename = "DECLARE")]
    Declare(RpcDeclareTransaction),
    #[serde(rename = "DEPLOY_ACCOUNT")]
    DeployAccount(RpcDeployAccountTransaction),
    #[serde(rename = "INVOKE")]
    Invoke(RpcInvokeTransaction),
}

macro_rules! implement_ref_getters {
    ($(($member_name:ident, $member_type:ty)), *) => {
        $(pub fn $member_name(&self) -> &$member_type {
            match self {
                RpcTransaction::Declare(
                    RpcDeclareTransaction::V3(tx)
                ) => &tx.$member_name,
                RpcTransaction::DeployAccount(
                    RpcDeployAccountTransaction::V3(tx)
                ) => &tx.$member_name,
                RpcTransaction::Invoke(
                    RpcInvokeTransaction::V3(tx)
                ) => &tx.$member_name
            }
        })*
    };
}

impl RpcTransaction {
    implement_ref_getters!(
        (nonce, Nonce),
        (resource_bounds, ResourceBoundsMapping),
        (signature, TransactionSignature),
        (tip, Tip)
    );

    pub fn calculate_sender_address(&self) -> Result<ContractAddress, StarknetApiError> {
        match self {
            RpcTransaction::Declare(RpcDeclareTransaction::V3(tx)) => Ok(tx.sender_address),
            RpcTransaction::DeployAccount(RpcDeployAccountTransaction::V3(tx)) => {
                calculate_contract_address(
                    tx.contract_address_salt,
                    tx.class_hash,
                    &tx.constructor_calldata,
                    ContractAddress::default(),
                )
            }
            RpcTransaction::Invoke(RpcInvokeTransaction::V3(tx)) => Ok(tx.sender_address),
        }
    }
}

/// A RPC declare transaction.
///
/// This transaction is equivalent to the component DECLARE_TXN in the
/// [`Starknet specs`] with a contract class (DECLARE_TXN allows having
/// either a contract class or a class hash).
///
/// [`Starknet specs`]: https://github.com/starkware-libs/starknet-specs/blob/master/api/starknet_api_openrpc.json
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "version")]
pub enum RpcDeclareTransaction {
    #[serde(rename = "0x3")]
    V3(RpcDeclareTransactionV3),
}

/// A RPC deploy account transaction.
///
/// This transaction is equivalent to the component DEPLOY_ACCOUNT_TXN in the
/// [`Starknet specs`].
///
/// [`Starknet specs`]: https://github.com/starkware-libs/starknet-specs/blob/master/api/starknet_api_openrpc.json
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "version")]
pub enum RpcDeployAccountTransaction {
    #[serde(rename = "0x3")]
    V3(RpcDeployAccountTransactionV3),
}

/// A RPC invoke transaction.
///
/// This transaction is equivalent to the component INVOKE_TXN in the
/// [`Starknet specs`].
///
/// [`Starknet specs`]: https://github.com/starkware-libs/starknet-specs/blob/master/api/starknet_api_openrpc.json
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "version")]
pub enum RpcInvokeTransaction {
    #[serde(rename = "0x3")]
    V3(RpcInvokeTransactionV3),
}

/// A declare transaction of a Cairo-v1 contract class that can be added to Starknet through the
/// RPC.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RpcDeclareTransactionV3 {
    // TODO: Check with Shahak why we need to keep the DeclareType.
    // pub r#type: DeclareType,
    pub sender_address: ContractAddress,
    pub compiled_class_hash: CompiledClassHash,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub contract_class: ContractClass,
    pub resource_bounds: ResourceBoundsMapping,
    pub tip: Tip,
    pub paymaster_data: PaymasterData,
    pub account_deployment_data: AccountDeploymentData,
    pub nonce_data_availability_mode: DataAvailabilityMode,
    pub fee_data_availability_mode: DataAvailabilityMode,
}

/// A deploy account transaction that can be added to Starknet through the RPC.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct RpcDeployAccountTransactionV3 {
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub class_hash: ClassHash,
    pub contract_address_salt: ContractAddressSalt,
    pub constructor_calldata: Calldata,
    pub resource_bounds: ResourceBoundsMapping,
    pub tip: Tip,
    pub paymaster_data: PaymasterData,
    pub nonce_data_availability_mode: DataAvailabilityMode,
    pub fee_data_availability_mode: DataAvailabilityMode,
}

/// An invoke account transaction that can be added to Starknet through the RPC.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct RpcInvokeTransactionV3 {
    pub sender_address: ContractAddress,
    pub calldata: Calldata,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub resource_bounds: ResourceBoundsMapping,
    pub tip: Tip,
    pub paymaster_data: PaymasterData,
    pub account_deployment_data: AccountDeploymentData,
    pub nonce_data_availability_mode: DataAvailabilityMode,
    pub fee_data_availability_mode: DataAvailabilityMode,
}

// The contract class in SN_API state doesn't have `contract_class_version`, not following the spec.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContractClass {
    pub sierra_program: Vec<Felt>,
    pub contract_class_version: String,
    pub entry_points_by_type: EntryPointByType,
    pub abi: String,
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct EntryPointByType {
    #[serde(rename = "CONSTRUCTOR")]
    pub constructor: Vec<EntryPoint>,
    #[serde(rename = "EXTERNAL")]
    pub external: Vec<EntryPoint>,
    #[serde(rename = "L1_HANDLER")]
    pub l1handler: Vec<EntryPoint>,
}

// The serialization of the struct in transaction is in capital letters, not following the spec.
#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ResourceBoundsMapping {
    pub l1_gas: ResourceBounds,
    pub l2_gas: ResourceBounds,
}

impl From<ResourceBoundsMapping> for crate::transaction::DeprecatedResourceBoundsMapping {
    fn from(mapping: ResourceBoundsMapping) -> crate::transaction::DeprecatedResourceBoundsMapping {
        let map =
            BTreeMap::from([(Resource::L1Gas, mapping.l1_gas), (Resource::L2Gas, mapping.l2_gas)]);
        crate::transaction::DeprecatedResourceBoundsMapping(map)
    }
}

impl From<ResourceBoundsMapping> for crate::transaction::ValidResourceBounds {
    fn from(value: ResourceBoundsMapping) -> Self {
        if !value.l2_gas.is_zero() {
            panic!("Resource bounds mapping l2 gas is expected to be zero. Got: {:?}", value.l2_gas)
        }
        Self::L1Gas(value.l1_gas)
    }
}

impl From<crate::transaction::ValidResourceBounds> for ResourceBoundsMapping {
    fn from(value: crate::transaction::ValidResourceBounds) -> Self {
        match value {
            crate::transaction::ValidResourceBounds::L1Gas(l1_gas) => {
                Self { l1_gas, l2_gas: ResourceBounds::default() }
            }
            crate::transaction::ValidResourceBounds::AllResources(_) => todo!(),
        }
    }
}
