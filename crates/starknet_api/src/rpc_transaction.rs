#[cfg(test)]
#[path = "rpc_transaction_test.rs"]
mod rpc_transaction_test;

use std::collections::HashMap;

use cairo_lang_starknet_classes::contract_class::ContractEntryPoints as CairoLangContractEntryPoints;
use serde::{Deserialize, Serialize};
use sizeof::SizeOf;
use strum::EnumVariantNames;
use strum_macros::{EnumDiscriminants, EnumIter, IntoStaticStr};

use crate::contract_class::EntryPointType;
use crate::core::{ChainId, ClassHash, CompiledClassHash, ContractAddress, Nonce};
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
    CalculateContractAddress,
    DeclareTransaction,
    DeclareTransactionV3,
    DeployAccountTransaction,
    DeployAccountTransactionV3,
    DeployTransactionTrait,
    InvokeTransaction,
    InvokeTransactionV3,
    Transaction,
    TransactionHash,
    TransactionHasher,
    TransactionVersion,
};
use crate::transaction_hash::{
    get_declare_transaction_v3_hash,
    get_deploy_account_transaction_v3_hash,
    get_invoke_transaction_v3_hash,
    DeclareTransactionV3Trait,
    DeployAccountTransactionV3Trait,
    InvokeTransactionV3Trait,
};
use crate::{impl_deploy_transaction_trait, StarknetApiError};

/// Transactions that are ready to be broadcasted to the network through RPC and are not included in
/// a block.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Hash, EnumDiscriminants)]
#[strum_discriminants(
    name(RpcTransactionLabelValue),
    derive(IntoStaticStr, EnumIter, EnumVariantNames),
    strum(serialize_all = "snake_case")
)]
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Hash, SizeOf)]
pub struct InternalRpcDeployAccountTransaction {
    pub tx: RpcDeployAccountTransaction,
    pub contract_address: ContractAddress,
}

impl InternalRpcDeployAccountTransaction {
    pub fn version(&self) -> TransactionVersion {
        self.tx.version()
    }
}

impl TransactionHasher for InternalRpcDeployAccountTransaction {
    fn calculate_transaction_hash(
        &self,
        chain_id: &ChainId,
        transaction_version: &TransactionVersion,
    ) -> Result<TransactionHash, StarknetApiError> {
        match &self.tx {
            RpcDeployAccountTransaction::V3(tx) => {
                tx.calculate_transaction_hash(chain_id, transaction_version)
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Hash, EnumDiscriminants, SizeOf)]
#[strum_discriminants(
    name(InternalRpcTransactionLabelValue),
    derive(IntoStaticStr, EnumIter, EnumVariantNames),
    strum(serialize_all = "snake_case")
)]
#[serde(tag = "type")]
#[serde(deny_unknown_fields)]
pub enum InternalRpcTransactionWithoutTxHash {
    #[serde(rename = "DECLARE")]
    Declare(InternalRpcDeclareTransactionV3),
    #[serde(rename = "DEPLOY_ACCOUNT")]
    DeployAccount(InternalRpcDeployAccountTransaction),
    #[serde(rename = "INVOKE")]
    Invoke(RpcInvokeTransaction),
}

impl InternalRpcTransactionWithoutTxHash {
    pub fn version(&self) -> TransactionVersion {
        match self {
            InternalRpcTransactionWithoutTxHash::Declare(tx) => tx.version(),
            InternalRpcTransactionWithoutTxHash::Invoke(tx) => tx.version(),
            InternalRpcTransactionWithoutTxHash::DeployAccount(tx) => tx.version(),
        }
    }

    pub fn calculate_transaction_hash(
        &self,
        chain_id: &ChainId,
    ) -> Result<TransactionHash, StarknetApiError> {
        let transaction_version = &self.version();
        match self {
            InternalRpcTransactionWithoutTxHash::Declare(tx) => {
                tx.calculate_transaction_hash(chain_id, transaction_version)
            }
            InternalRpcTransactionWithoutTxHash::Invoke(tx) => {
                tx.calculate_transaction_hash(chain_id, transaction_version)
            }
            InternalRpcTransactionWithoutTxHash::DeployAccount(tx) => {
                tx.calculate_transaction_hash(chain_id, transaction_version)
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Hash, SizeOf)]
pub struct InternalRpcTransaction {
    pub tx: InternalRpcTransactionWithoutTxHash,
    pub tx_hash: TransactionHash,
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
                tx.calculate_contract_address()
            }
            RpcTransaction::Invoke(RpcInvokeTransaction::V3(tx)) => Ok(tx.sender_address),
        }
    }
}

// TODO(Arni): Replace this with RPCTransaction -> InternalRpcTransaction conversion (don't use From
// because it contains hash calculations).
impl From<RpcTransaction> for Transaction {
    fn from(rpc_transaction: RpcTransaction) -> Self {
        match rpc_transaction {
            RpcTransaction::Declare(tx) => Transaction::Declare(tx.into()),
            RpcTransaction::DeployAccount(tx) => Transaction::DeployAccount(tx.into()),
            RpcTransaction::Invoke(tx) => Transaction::Invoke(tx.into()),
        }
    }
}

macro_rules! implement_internal_getters_for_internal_rpc {
    ($(($field_name:ident, $field_ty:ty)),* $(,)?) => {
        $(
            pub fn $field_name(&self) -> $field_ty {
                match &self.tx {
                    InternalRpcTransactionWithoutTxHash::Declare(tx) => tx.$field_name.clone(),
                    InternalRpcTransactionWithoutTxHash::DeployAccount(tx) => {
                        let RpcDeployAccountTransaction::V3(tx) = &tx.tx;
                        tx.$field_name.clone()
                    },
                    InternalRpcTransactionWithoutTxHash::Invoke(RpcInvokeTransaction::V3(tx)) => tx.$field_name.clone(),
                }
            }
        )*
    };
}

impl InternalRpcTransaction {
    implement_internal_getters_for_internal_rpc!(
        (nonce, Nonce),
        (resource_bounds, AllResourceBounds),
        (tip, Tip),
    );

    pub fn contract_address(&self) -> ContractAddress {
        match &self.tx {
            InternalRpcTransactionWithoutTxHash::Declare(tx) => tx.sender_address,
            InternalRpcTransactionWithoutTxHash::DeployAccount(tx) => tx.contract_address,
            InternalRpcTransactionWithoutTxHash::Invoke(RpcInvokeTransaction::V3(tx)) => {
                tx.sender_address
            }
        }
    }

    pub fn total_bytes(&self) -> u64 {
        self.size_bytes()
            .try_into()
            .expect("The transaction size in bytes should fit in a u64 value.")
    }

    pub fn tx_hash(&self) -> TransactionHash {
        self.tx_hash
    }
}
/// A RPC declare transaction.
///
/// This transaction is equivalent to the component DECLARE_TXN in the
/// [`Starknet specs`] with a contract class (DECLARE_TXN allows having
/// either a contract class or a class hash).
///
/// [`Starknet specs`]: https://github.com/starkware-libs/starknet-specs/blob/master/api/starknet_api_openrpc.json
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Hash)]
#[serde(tag = "version")]
pub enum RpcDeclareTransaction {
    #[serde(rename = "0x3")]
    V3(RpcDeclareTransactionV3),
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
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, SizeOf)]
#[serde(tag = "version")]
pub enum RpcDeployAccountTransaction {
    #[serde(rename = "0x3")]
    V3(RpcDeployAccountTransactionV3),
}

impl RpcDeployAccountTransaction {
    fn version(&self) -> TransactionVersion {
        match self {
            RpcDeployAccountTransaction::V3(_) => TransactionVersion::THREE,
        }
    }
}

impl From<RpcDeployAccountTransaction> for DeployAccountTransaction {
    fn from(rpc_deploy_account_transaction: RpcDeployAccountTransaction) -> Self {
        match rpc_deploy_account_transaction {
            RpcDeployAccountTransaction::V3(tx) => DeployAccountTransaction::V3(tx.into()),
        }
    }
}

impl TryFrom<DeployAccountTransactionV3> for RpcDeployAccountTransactionV3 {
    type Error = StarknetApiError;

    fn try_from(value: DeployAccountTransactionV3) -> Result<Self, Self::Error> {
        Ok(Self {
            resource_bounds: match value.resource_bounds {
                ValidResourceBounds::AllResources(bounds) => bounds,
                _ => {
                    return Err(StarknetApiError::OutOfRange {
                        string: "resource_bounds".to_string(),
                    });
                }
            },
            signature: value.signature,
            nonce: value.nonce,
            class_hash: value.class_hash,
            contract_address_salt: value.contract_address_salt,
            constructor_calldata: value.constructor_calldata,
            tip: value.tip,
            paymaster_data: value.paymaster_data,
            nonce_data_availability_mode: value.nonce_data_availability_mode,
            fee_data_availability_mode: value.fee_data_availability_mode,
        })
    }
}

/// A RPC invoke transaction.
///
/// This transaction is equivalent to the component INVOKE_TXN in the
/// [`Starknet specs`].
///
/// [`Starknet specs`]: https://github.com/starkware-libs/starknet-specs/blob/master/api/starknet_api_openrpc.json
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, SizeOf)]
#[serde(tag = "version")]
pub enum RpcInvokeTransaction {
    #[serde(rename = "0x3")]
    V3(RpcInvokeTransactionV3),
}

impl RpcInvokeTransaction {
    pub fn version(&self) -> TransactionVersion {
        match self {
            RpcInvokeTransaction::V3(_) => TransactionVersion::THREE,
        }
    }
}

impl TransactionHasher for RpcInvokeTransaction {
    fn calculate_transaction_hash(
        &self,
        chain_id: &ChainId,
        transaction_version: &TransactionVersion,
    ) -> Result<TransactionHash, StarknetApiError> {
        match self {
            RpcInvokeTransaction::V3(tx) => {
                tx.calculate_transaction_hash(chain_id, transaction_version)
            }
        }
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
    // TODO(Mohammad): Check with Shahak why we need to keep the DeclareType.
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
            class_hash: tx.contract_class.calculate_class_hash(),
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

/// An [RpcDeclareTransactionV3] that contains a class hash instead of the full contract class.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Hash, SizeOf)]
pub struct InternalRpcDeclareTransactionV3 {
    pub sender_address: ContractAddress,
    pub compiled_class_hash: CompiledClassHash,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub class_hash: ClassHash,
    pub resource_bounds: AllResourceBounds,
    pub tip: Tip,
    pub paymaster_data: PaymasterData,
    pub account_deployment_data: AccountDeploymentData,
    pub nonce_data_availability_mode: DataAvailabilityMode,
    pub fee_data_availability_mode: DataAvailabilityMode,
}

impl InternalRpcDeclareTransactionV3 {
    pub fn version(&self) -> TransactionVersion {
        TransactionVersion::THREE
    }
}

impl DeclareTransactionV3Trait for InternalRpcDeclareTransactionV3 {
    fn resource_bounds(&self) -> ValidResourceBounds {
        ValidResourceBounds::AllResources(self.resource_bounds)
    }
    fn tip(&self) -> &Tip {
        &self.tip
    }
    fn paymaster_data(&self) -> &PaymasterData {
        &self.paymaster_data
    }
    fn nonce_data_availability_mode(&self) -> &DataAvailabilityMode {
        &self.nonce_data_availability_mode
    }
    fn fee_data_availability_mode(&self) -> &DataAvailabilityMode {
        &self.fee_data_availability_mode
    }
    fn account_deployment_data(&self) -> &AccountDeploymentData {
        &self.account_deployment_data
    }
    fn sender_address(&self) -> &ContractAddress {
        &self.sender_address
    }
    fn nonce(&self) -> &Nonce {
        &self.nonce
    }
    fn class_hash(&self) -> &ClassHash {
        &self.class_hash
    }
    fn compiled_class_hash(&self) -> &CompiledClassHash {
        &self.compiled_class_hash
    }
}

impl TransactionHasher for InternalRpcDeclareTransactionV3 {
    fn calculate_transaction_hash(
        &self,
        chain_id: &ChainId,
        transaction_version: &TransactionVersion,
    ) -> Result<TransactionHash, StarknetApiError> {
        get_declare_transaction_v3_hash(self, chain_id, transaction_version)
    }
}

impl From<InternalRpcDeclareTransactionV3> for DeclareTransactionV3 {
    fn from(tx: InternalRpcDeclareTransactionV3) -> Self {
        Self {
            class_hash: tx.class_hash,
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

impl From<InternalRpcDeclareTransactionV3> for DeclareTransaction {
    fn from(tx: InternalRpcDeclareTransactionV3) -> Self {
        Self::V3(tx.into())
    }
}

/// A deploy account transaction that can be added to Starknet through the RPC.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, SizeOf)]
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

impl_deploy_transaction_trait!(RpcDeployAccountTransactionV3);

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

impl DeployAccountTransactionV3Trait for RpcDeployAccountTransactionV3 {
    fn resource_bounds(&self) -> ValidResourceBounds {
        ValidResourceBounds::AllResources(self.resource_bounds)
    }
    fn tip(&self) -> &Tip {
        &self.tip
    }
    fn paymaster_data(&self) -> &PaymasterData {
        &self.paymaster_data
    }
    fn nonce_data_availability_mode(&self) -> &DataAvailabilityMode {
        &self.nonce_data_availability_mode
    }
    fn fee_data_availability_mode(&self) -> &DataAvailabilityMode {
        &self.fee_data_availability_mode
    }
    fn constructor_calldata(&self) -> &Calldata {
        &self.constructor_calldata
    }
    fn nonce(&self) -> &Nonce {
        &self.nonce
    }
    fn class_hash(&self) -> &ClassHash {
        &self.class_hash
    }
    fn contract_address_salt(&self) -> &ContractAddressSalt {
        &self.contract_address_salt
    }
}

impl TransactionHasher for RpcDeployAccountTransactionV3 {
    fn calculate_transaction_hash(
        &self,
        chain_id: &ChainId,
        transaction_version: &TransactionVersion,
    ) -> Result<TransactionHash, StarknetApiError> {
        get_deploy_account_transaction_v3_hash(self, chain_id, transaction_version)
    }
}

/// An invoke account transaction that can be added to Starknet through the RPC.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, SizeOf)]
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

impl InvokeTransactionV3Trait for RpcInvokeTransactionV3 {
    fn resource_bounds(&self) -> ValidResourceBounds {
        ValidResourceBounds::AllResources(self.resource_bounds)
    }
    fn tip(&self) -> &Tip {
        &self.tip
    }
    fn paymaster_data(&self) -> &PaymasterData {
        &self.paymaster_data
    }
    fn nonce_data_availability_mode(&self) -> &DataAvailabilityMode {
        &self.nonce_data_availability_mode
    }
    fn fee_data_availability_mode(&self) -> &DataAvailabilityMode {
        &self.fee_data_availability_mode
    }
    fn account_deployment_data(&self) -> &AccountDeploymentData {
        &self.account_deployment_data
    }
    fn sender_address(&self) -> &ContractAddress {
        &self.sender_address
    }
    fn nonce(&self) -> &Nonce {
        &self.nonce
    }
    fn calldata(&self) -> &Calldata {
        &self.calldata
    }
}

impl TransactionHasher for RpcInvokeTransactionV3 {
    fn calculate_transaction_hash(
        &self,
        chain_id: &ChainId,
        transaction_version: &TransactionVersion,
    ) -> Result<TransactionHash, StarknetApiError> {
        get_invoke_transaction_v3_hash(self, chain_id, transaction_version)
    }
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

impl TryFrom<InvokeTransactionV3> for RpcInvokeTransactionV3 {
    type Error = StarknetApiError;

    fn try_from(value: InvokeTransactionV3) -> Result<Self, Self::Error> {
        Ok(Self {
            resource_bounds: match value.resource_bounds {
                ValidResourceBounds::AllResources(bounds) => bounds,
                _ => {
                    return Err(StarknetApiError::OutOfRange {
                        string: "resource_bounds".to_string(),
                    });
                }
            },
            signature: value.signature,
            nonce: value.nonce,
            tip: value.tip,
            paymaster_data: value.paymaster_data,
            nonce_data_availability_mode: value.nonce_data_availability_mode,
            fee_data_availability_mode: value.fee_data_availability_mode,
            sender_address: value.sender_address,
            calldata: value.calldata,
            account_deployment_data: value.account_deployment_data,
        })
    }
}

// TODO(Aviv): remove duplication with sequencer/crates/apollo_rpc/src/v0_8/state.rs
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize, Hash)]
pub struct EntryPointByType {
    #[serde(rename = "CONSTRUCTOR")]
    pub constructor: Vec<EntryPoint>,
    #[serde(rename = "EXTERNAL")]
    pub external: Vec<EntryPoint>,
    #[serde(rename = "L1_HANDLER")]
    pub l1handler: Vec<EntryPoint>,
}

// TODO(AVIV): Consider removing this conversion and using CairoLangContractEntryPoints instead of
// defining the EntryPointByType struct.
impl From<CairoLangContractEntryPoints> for EntryPointByType {
    fn from(value: CairoLangContractEntryPoints) -> Self {
        Self {
            constructor: value.constructor.into_iter().map(EntryPoint::from).collect(),
            external: value.external.into_iter().map(EntryPoint::from).collect(),
            l1handler: value.l1_handler.into_iter().map(EntryPoint::from).collect(),
        }
    }
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
