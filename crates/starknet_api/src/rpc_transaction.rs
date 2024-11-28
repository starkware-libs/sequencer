#[cfg(test)]
#[path = "rpc_transaction_test.rs"]
mod rpc_transaction_test;
use std::collections::HashMap;
use std::fmt;

use serde::de::{self, MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::contract_class::EntryPointType;
use crate::core::{
    calculate_contract_address,
    ClassHash,
    CompiledClassHash,
    ContractAddress,
    Nonce,
};
use crate::data_availability::DataAvailabilityMode;
use crate::state::{EntryPoint, SierraContractClass};
use crate::transaction::fields::{
    AccountDeploymentData,
    AllResourceBounds,
    Calldata,
    ContractAddressSalt,
    PaymasterData,
    Tip,
    TransactionSignature,
    ValidResourceBounds,
};
use crate::transaction::{
    DeclareTransaction,
    DeclareTransactionV3,
    DeployAccountTransaction,
    DeployAccountTransactionV3,
    InvokeTransaction,
    InvokeTransactionV3,
    Transaction,
};
use crate::StarknetApiError;

/// Transactions that are ready to be broadcasted to the network through RPC and are not included in
/// a block.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Hash)]
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
        (tip, Tip),
        (nonce_data_availability_mode, DataAvailabilityMode),
        (fee_data_availability_mode, DataAvailabilityMode)
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

impl From<RpcTransaction> for Transaction {
    fn from(rpc_transaction: RpcTransaction) -> Self {
        match rpc_transaction {
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
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum RpcDeclareTransaction {
    V3(RpcDeclareTransactionV3),
}

impl Serialize for RpcDeclareTransaction {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(Some(2))?; // 2 fields: version + data
        match self {
            RpcDeclareTransaction::V3(data) => {
                state.serialize_entry("version", "0x3")?;
                state.serialize_entry("data", data)?;
            }
        }
        state.end()
    }
}

impl<'de> Deserialize<'de> for RpcDeclareTransaction {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct RpcDeclareTransactionVisitor;

        impl<'de> Visitor<'de> for RpcDeclareTransactionVisitor {
            type Value = RpcDeclareTransaction;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a map with version and data fields")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut version: Option<String> = None;
                let mut data: Option<RpcDeclareTransactionV3> = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "version" => {
                            if version.is_some() {
                                return Err(de::Error::duplicate_field("version"));
                            }
                            version = Some(map.next_value()?);
                        }
                        "data" => {
                            if data.is_some() {
                                return Err(de::Error::duplicate_field("data"));
                            }
                            data = Some(map.next_value()?);
                        }
                        _ => {
                            return Err(de::Error::unknown_field(&key, &["version", "data"]));
                        }
                    }
                }

                let version = version.ok_or_else(|| de::Error::missing_field("version"))?;
                let data = data.ok_or_else(|| de::Error::missing_field("data"))?;

                match version.as_str() {
                    "0x3" => Ok(RpcDeclareTransaction::V3(data)),
                    _ => Err(de::Error::unknown_variant(&version, &["0x3"])),
                }
            }
        }

        deserializer.deserialize_map(RpcDeclareTransactionVisitor)
    }
}


impl From<RpcDeclareTransaction> for DeclareTransaction {
    fn from(rpc_declare_transaction: RpcDeclareTransaction) -> Self {
        match rpc_declare_transaction {
            RpcDeclareTransaction::V3(tx) => DeclareTransaction::V3(tx.into()),
        }
    }
}

/// A RPC deploy account transaction.
///
/// This transaction is equivalent to the component DEPLOY_ACCOUNT_TXN in the
/// [`Starknet specs`].
///
/// [`Starknet specs`]: https://github.com/starkware-libs/starknet-specs/blob/master/api/starknet_api_openrpc.json
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RpcDeployAccountTransaction {
    V3(RpcDeployAccountTransactionV3),
}

impl Serialize for RpcDeployAccountTransaction {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(Some(2))?; // 2 fields: version + data
        match self {
            RpcDeployAccountTransaction::V3(data) => {
                state.serialize_entry("version", "0x3")?;
                state.serialize_entry("data", data)?;
            }
        }
        state.end()
    }
}

impl<'de> Deserialize<'de> for RpcDeployAccountTransaction {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct RpcDeployAccountTransactionVisitor;

        impl<'de> Visitor<'de> for RpcDeployAccountTransactionVisitor {
            type Value = RpcDeployAccountTransaction;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a map with version and data fields")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut version: Option<String> = None;
                let mut data: Option<RpcDeployAccountTransactionV3> = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "version" => {
                            if version.is_some() {
                                return Err(de::Error::duplicate_field("version"));
                            }
                            version = Some(map.next_value()?);
                        }
                        "data" => {
                            if data.is_some() {
                                return Err(de::Error::duplicate_field("data"));
                            }
                            data = Some(map.next_value()?);
                        }
                        _ => {
                            return Err(de::Error::unknown_field(&key, &["version", "data"]));
                        }
                    }
                }

                let version = version.ok_or_else(|| de::Error::missing_field("version"))?;
                let data = data.ok_or_else(|| de::Error::missing_field("data"))?;

                match version.as_str() {
                    "0x3" => Ok(RpcDeployAccountTransaction::V3(data)),
                    _ => Err(de::Error::unknown_variant(&version, &["0x3"])),
                }
            }
        }

        deserializer.deserialize_map(RpcDeployAccountTransactionVisitor)
    }
}

impl From<RpcDeployAccountTransaction> for DeployAccountTransaction {
    fn from(rpc_deploy_account_transaction: RpcDeployAccountTransaction) -> Self {
        match rpc_deploy_account_transaction {
            RpcDeployAccountTransaction::V3(tx) => DeployAccountTransaction::V3(tx.into()),
        }
    }
}

/// A RPC invoke transaction.
///
/// This transaction is equivalent to the component INVOKE_TXN in the
/// [`Starknet specs`].
///
/// [`Starknet specs`]: https://github.com/starkware-libs/starknet-specs/blob/master/api/starknet_api_openrpc.json
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RpcInvokeTransaction {
    V3(RpcInvokeTransactionV3),
}

impl Serialize for RpcInvokeTransaction {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(Some(2))?; // 2 fields: version + data
        match self {
            RpcInvokeTransaction::V3(data) => {
                state.serialize_entry("version", "0x3")?;
                state.serialize_entry("data", data)?;
            }
        }
        state.end()
    }
}

impl<'de> Deserialize<'de> for RpcInvokeTransaction {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct RpcInvokeTransactionVisitor;

        impl<'de> Visitor<'de> for RpcInvokeTransactionVisitor {
            type Value = RpcInvokeTransaction;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a map with version and data fields")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut version: Option<String> = None;
                let mut data: Option<RpcInvokeTransactionV3> = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "version" => {
                            if version.is_some() {
                                return Err(de::Error::duplicate_field("version"));
                            }
                            version = Some(map.next_value()?);
                        }
                        "data" => {
                            if data.is_some() {
                                return Err(de::Error::duplicate_field("data"));
                            }
                            data = Some(map.next_value()?);
                        }
                        _ => {
                            return Err(de::Error::unknown_field(&key, &["version", "data"]));
                        }
                    }
                }

                let version = version.ok_or_else(|| de::Error::missing_field("version"))?;
                let data = data.ok_or_else(|| de::Error::missing_field("data"))?;

                match version.as_str() {
                    "0x3" => Ok(RpcInvokeTransaction::V3(data)),
                    _ => Err(de::Error::unknown_variant(&version, &["0x3"])),
                }
            }
        }

        deserializer.deserialize_map(RpcInvokeTransactionVisitor)
    }
}

impl From<RpcInvokeTransaction> for InvokeTransaction {
    fn from(rpc_invoke_tx: RpcInvokeTransaction) -> Self {
        match rpc_invoke_tx {
            RpcInvokeTransaction::V3(tx) => InvokeTransaction::V3(tx.into()),
        }
    }
}

/// A declare transaction of a Cairo-v1 contract class that can be added to Starknet through the
/// RPC.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Hash)]
pub struct RpcDeclareTransactionV3 {
    // TODO: Check with Shahak why we need to keep the DeclareType.
    // pub r#type: DeclareType,
    pub sender_address: ContractAddress,
    pub compiled_class_hash: CompiledClassHash,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub contract_class: SierraContractClass,
    pub resource_bounds: AllResourceBounds,
    pub tip: Tip,
    pub paymaster_data: PaymasterData,
    pub account_deployment_data: AccountDeploymentData,
    pub nonce_data_availability_mode: DataAvailabilityMode,
    pub fee_data_availability_mode: DataAvailabilityMode,
}

impl From<RpcDeclareTransactionV3> for DeclareTransactionV3 {
    fn from(tx: RpcDeclareTransactionV3) -> Self {
        Self {
            class_hash: ClassHash::default(), /* FIXME(yael 15/4/24): call the starknet-api
                                               * function once ready */
            resource_bounds: ValidResourceBounds::AllResources(tx.resource_bounds),
            tip: tx.tip,
            signature: tx.signature,
            nonce: tx.nonce,
            compiled_class_hash: tx.compiled_class_hash,
            sender_address: tx.sender_address,
            nonce_data_availability_mode: tx.nonce_data_availability_mode,
            fee_data_availability_mode: tx.fee_data_availability_mode,
            paymaster_data: tx.paymaster_data,
            account_deployment_data: tx.account_deployment_data,
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

impl From<RpcDeployAccountTransactionV3> for DeployAccountTransactionV3 {
    fn from(tx: RpcDeployAccountTransactionV3) -> Self {
        Self {
            resource_bounds: ValidResourceBounds::AllResources(tx.resource_bounds),
            tip: tx.tip,
            signature: tx.signature,
            nonce: tx.nonce,
            class_hash: tx.class_hash,
            contract_address_salt: tx.contract_address_salt,
            constructor_calldata: tx.constructor_calldata,
            nonce_data_availability_mode: tx.nonce_data_availability_mode,
            fee_data_availability_mode: tx.fee_data_availability_mode,
            paymaster_data: tx.paymaster_data,
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

impl From<RpcInvokeTransactionV3> for InvokeTransactionV3 {
    fn from(tx: RpcInvokeTransactionV3) -> Self {
        Self {
            resource_bounds: ValidResourceBounds::AllResources(tx.resource_bounds),
            tip: tx.tip,
            signature: tx.signature,
            nonce: tx.nonce,
            sender_address: tx.sender_address,
            calldata: tx.calldata,
            nonce_data_availability_mode: tx.nonce_data_availability_mode,
            fee_data_availability_mode: tx.fee_data_availability_mode,
            paymaster_data: tx.paymaster_data,
            account_deployment_data: tx.account_deployment_data,
        }
    }
}

// TODO(Aviv): remove duplication with sequencer/crates/papyrus_rpc/src/v0_8/state.rs
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize, Hash)]
pub struct EntryPointByType {
    #[serde(rename = "CONSTRUCTOR")]
    pub constructor: Vec<EntryPoint>,
    #[serde(rename = "EXTERNAL")]
    pub external: Vec<EntryPoint>,
    #[serde(rename = "L1_HANDLER")]
    pub l1handler: Vec<EntryPoint>,
}

impl EntryPointByType {
    pub fn from_hash_map(entry_points_by_type: HashMap<EntryPointType, Vec<EntryPoint>>) -> Self {
        macro_rules! get_entrypoint_by_type {
            ($variant:ident) => {
                (*(entry_points_by_type.get(&EntryPointType::$variant).unwrap_or(&vec![]))).to_vec()
            };
        }

        Self {
            constructor: get_entrypoint_by_type!(Constructor),
            external: get_entrypoint_by_type!(External),
            l1handler: get_entrypoint_by_type!(L1Handler),
        }
    }
    pub fn to_hash_map(&self) -> HashMap<EntryPointType, Vec<EntryPoint>> {
        HashMap::from_iter([
            (EntryPointType::Constructor, self.constructor.clone()),
            (EntryPointType::External, self.external.clone()),
            (EntryPointType::L1Handler, self.l1handler.clone()),
        ])
    }
}
