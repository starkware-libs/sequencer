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
    AllResourceBounds,
    Calldata,
    ContractAddressSalt,
    DeclareTransaction,
    DeclareTransactionV3,
    DeployAccountTransaction,
    DeployAccountTransactionV3,
    InvokeTransaction,
    InvokeTransactionV3,
    PaymasterData,
    Resource,
    Tip,
    Transaction,
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
        (resource_bounds, AllResourceBounds),
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

impl Into<Transaction> for RpcTransaction {
    fn into(self) -> Transaction {
        match self {
            RpcTransaction::Declare(tx) => Transaction::Declare(tx.into()),
            RpcTransaction::DeployAccount(tx) => Transaction::DeployAccount(tx.into()),
            RpcTransaction::Invoke(tx) => Transaction::Invoke(tx.into()),
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

impl Into<DeclareTransaction> for RpcDeclareTransaction {
    fn into(self) -> DeclareTransaction {
        let RpcDeclareTransaction::V3(tx) = self;
        DeclareTransaction::V3(tx.into())
    }
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

impl Into<DeployAccountTransaction> for RpcDeployAccountTransaction {
    fn into(self) -> DeployAccountTransaction {
        let RpcDeployAccountTransaction::V3(tx) = self;
        DeployAccountTransaction::V3(tx.into())
    }
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

impl Into<InvokeTransaction> for RpcInvokeTransaction {
    fn into(self) -> InvokeTransaction {
        let RpcInvokeTransaction::V3(tx) = self;
        InvokeTransaction::V3(tx.into())
    }
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
    pub resource_bounds: AllResourceBounds,
    pub tip: Tip,
    pub paymaster_data: PaymasterData,
    pub account_deployment_data: AccountDeploymentData,
    pub nonce_data_availability_mode: DataAvailabilityMode,
    pub fee_data_availability_mode: DataAvailabilityMode,
}

impl Into<DeclareTransactionV3> for RpcDeclareTransactionV3 {
    fn into(self) -> DeclareTransactionV3 {
        DeclareTransactionV3 {
            class_hash: ClassHash::default(), /* FIXME(yael 15/4/24): call the starknet-api
                                               * function once ready */
            resource_bounds: self.resource_bounds.into(),
            tip: self.tip,
            signature: self.signature,
            nonce: self.nonce,
            compiled_class_hash: self.compiled_class_hash,
            sender_address: self.sender_address,
            nonce_data_availability_mode: self.nonce_data_availability_mode,
            fee_data_availability_mode: self.fee_data_availability_mode,
            paymaster_data: self.paymaster_data,
            account_deployment_data: self.account_deployment_data,
        }
    }
}

/// A deploy account transaction that can be added to Starknet through the RPC.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct RpcDeployAccountTransactionV3 {
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub class_hash: ClassHash,
    pub contract_address_salt: ContractAddressSalt,
    pub constructor_calldata: Calldata,
    pub resource_bounds: AllResourceBounds,
    pub tip: Tip,
    pub paymaster_data: PaymasterData,
    pub nonce_data_availability_mode: DataAvailabilityMode,
    pub fee_data_availability_mode: DataAvailabilityMode,
}

impl Into<DeployAccountTransactionV3> for RpcDeployAccountTransactionV3 {
    fn into(self) -> DeployAccountTransactionV3 {
        DeployAccountTransactionV3 {
            resource_bounds: self.resource_bounds.into(),
            tip: self.tip,
            signature: self.signature,
            nonce: self.nonce,
            class_hash: self.class_hash,
            contract_address_salt: self.contract_address_salt,
            constructor_calldata: self.constructor_calldata,
            nonce_data_availability_mode: self.nonce_data_availability_mode,
            fee_data_availability_mode: self.fee_data_availability_mode,
            paymaster_data: self.paymaster_data,
        }
    }
}

/// An invoke account transaction that can be added to Starknet through the RPC.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct RpcInvokeTransactionV3 {
    pub sender_address: ContractAddress,
    pub calldata: Calldata,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub resource_bounds: AllResourceBounds,
    pub tip: Tip,
    pub paymaster_data: PaymasterData,
    pub account_deployment_data: AccountDeploymentData,
    pub nonce_data_availability_mode: DataAvailabilityMode,
    pub fee_data_availability_mode: DataAvailabilityMode,
}

impl Into<InvokeTransactionV3> for RpcInvokeTransactionV3 {
    fn into(self) -> InvokeTransactionV3 {
        InvokeTransactionV3 {
            resource_bounds: self.resource_bounds.into(),
            tip: self.tip,
            signature: self.signature,
            nonce: self.nonce,
            sender_address: self.sender_address,
            calldata: self.calldata,
            nonce_data_availability_mode: self.nonce_data_availability_mode,
            fee_data_availability_mode: self.fee_data_availability_mode,
            paymaster_data: self.paymaster_data,
            account_deployment_data: self.account_deployment_data,
        }
    }
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

// TODO(Nimrod): Remove this conversion.
impl From<AllResourceBounds> for crate::transaction::DeprecatedResourceBoundsMapping {
    fn from(
        all_resource_bounds: AllResourceBounds,
    ) -> crate::transaction::DeprecatedResourceBoundsMapping {
        let map = BTreeMap::from([
            (Resource::L1Gas, all_resource_bounds.l1_gas),
            (Resource::L2Gas, all_resource_bounds.l2_gas),
            (Resource::L1DataGas, all_resource_bounds.l1_data_gas),
        ]);
        crate::transaction::DeprecatedResourceBoundsMapping(map)
    }
}
