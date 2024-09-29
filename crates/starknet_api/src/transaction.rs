use std::collections::BTreeMap;
use std::fmt::Display;
use std::sync::Arc;

use derive_more::{Display, From};
use num_bigint::BigUint;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use starknet_types_core::felt::Felt;
use strum_macros::EnumIter;

use crate::block::{BlockHash, BlockNumber, GasPrice, NonzeroGasPrice};
use crate::core::{
    ChainId,
    ClassHash,
    CompiledClassHash,
    ContractAddress,
    EntryPointSelector,
    EthAddress,
    Nonce,
};
use crate::data_availability::DataAvailabilityMode;
use crate::execution_resources::{ExecutionResources, GasAmount};
use crate::hash::StarkHash;
use crate::serde_utils::PrefixedBytesAsHex;
use crate::transaction_hash::{
    get_declare_transaction_v0_hash,
    get_declare_transaction_v1_hash,
    get_declare_transaction_v2_hash,
    get_declare_transaction_v3_hash,
    get_deploy_account_transaction_v1_hash,
    get_deploy_account_transaction_v3_hash,
    get_deploy_transaction_hash,
    get_invoke_transaction_v0_hash,
    get_invoke_transaction_v1_hash,
    get_invoke_transaction_v3_hash,
    get_l1_handler_transaction_hash,
};
use crate::StarknetApiError;

#[cfg(test)]
#[path = "transaction_test.rs"]
mod transaction_test;

// TODO(Noa, 14/11/2023): Replace QUERY_VERSION_BASE_BIT with a lazy calculation.
//      pub static QUERY_VERSION_BASE: Lazy<Felt> = ...
pub const QUERY_VERSION_BASE_BIT: u32 = 128;

pub trait TransactionHasher {
    fn calculate_transaction_hash(
        &self,
        chain_id: &ChainId,
        transaction_version: &TransactionVersion,
    ) -> Result<TransactionHash, StarknetApiError>;
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct FullTransaction {
    pub transaction: Transaction,
    pub transaction_output: TransactionOutput,
    pub transaction_hash: TransactionHash,
}

/// A transaction.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum Transaction {
    /// A declare transaction.
    Declare(DeclareTransaction),
    /// A deploy transaction.
    Deploy(DeployTransaction),
    /// A deploy account transaction.
    DeployAccount(DeployAccountTransaction),
    /// An invoke transaction.
    Invoke(InvokeTransaction),
    /// An L1 handler transaction.
    L1Handler(L1HandlerTransaction),
}

impl Transaction {
    pub fn version(&self) -> TransactionVersion {
        match self {
            Transaction::Declare(tx) => tx.version(),
            Transaction::Deploy(tx) => tx.version,
            Transaction::DeployAccount(tx) => tx.version(),
            Transaction::Invoke(tx) => tx.version(),
            Transaction::L1Handler(tx) => tx.version,
        }
    }

    pub fn calculate_transaction_hash(
        &self,
        chain_id: &ChainId,
    ) -> Result<TransactionHash, StarknetApiError> {
        let transaction_version = &self.version();
        match self {
            Transaction::Declare(tx) => {
                tx.calculate_transaction_hash(chain_id, transaction_version)
            }
            Transaction::Deploy(tx) => tx.calculate_transaction_hash(chain_id, transaction_version),
            Transaction::DeployAccount(tx) => {
                tx.calculate_transaction_hash(chain_id, transaction_version)
            }
            Transaction::Invoke(tx) => tx.calculate_transaction_hash(chain_id, transaction_version),
            Transaction::L1Handler(tx) => {
                tx.calculate_transaction_hash(chain_id, transaction_version)
            }
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub struct TransactionOptions {
    /// Transaction that shouldn't be broadcasted to StarkNet. For example, users that want to
    /// test the execution result of a transaction without the risk of it being rebroadcasted (the
    /// signature will be different while the execution remain the same). Using this flag will
    /// modify the transaction version by setting the 128-th bit to 1.
    pub only_query: bool,
}

macro_rules! implement_v3_tx_getters {
    ($(($field:ident, $field_type:ty)),*) => {
        $(pub fn $field(&self) -> $field_type {
            match self {
                Self::V3(tx) => tx.$field.clone(),
                _ => panic!("{:?} do not support the field {}; they are only available for V3 transactions.", self.version(), stringify!($field)),
            }
        })*
    };
}

/// A transaction output.
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub enum TransactionOutput {
    /// A declare transaction output.
    Declare(DeclareTransactionOutput),
    /// A deploy transaction output.
    Deploy(DeployTransactionOutput),
    /// A deploy account transaction output.
    DeployAccount(DeployAccountTransactionOutput),
    /// An invoke transaction output.
    Invoke(InvokeTransactionOutput),
    /// An L1 handler transaction output.
    L1Handler(L1HandlerTransactionOutput),
}

impl TransactionOutput {
    pub fn actual_fee(&self) -> Fee {
        match self {
            TransactionOutput::Declare(output) => output.actual_fee,
            TransactionOutput::Deploy(output) => output.actual_fee,
            TransactionOutput::DeployAccount(output) => output.actual_fee,
            TransactionOutput::Invoke(output) => output.actual_fee,
            TransactionOutput::L1Handler(output) => output.actual_fee,
        }
    }

    pub fn events(&self) -> &[Event] {
        match self {
            TransactionOutput::Declare(output) => &output.events,
            TransactionOutput::Deploy(output) => &output.events,
            TransactionOutput::DeployAccount(output) => &output.events,
            TransactionOutput::Invoke(output) => &output.events,
            TransactionOutput::L1Handler(output) => &output.events,
        }
    }

    pub fn execution_status(&self) -> &TransactionExecutionStatus {
        match self {
            TransactionOutput::Declare(output) => &output.execution_status,
            TransactionOutput::Deploy(output) => &output.execution_status,
            TransactionOutput::DeployAccount(output) => &output.execution_status,
            TransactionOutput::Invoke(output) => &output.execution_status,
            TransactionOutput::L1Handler(output) => &output.execution_status,
        }
    }

    pub fn execution_resources(&self) -> &ExecutionResources {
        match self {
            TransactionOutput::Declare(output) => &output.execution_resources,
            TransactionOutput::Deploy(output) => &output.execution_resources,
            TransactionOutput::DeployAccount(output) => &output.execution_resources,
            TransactionOutput::Invoke(output) => &output.execution_resources,
            TransactionOutput::L1Handler(output) => &output.execution_resources,
        }
    }

    pub fn messages_sent(&self) -> &Vec<MessageToL1> {
        match self {
            TransactionOutput::Declare(output) => &output.messages_sent,
            TransactionOutput::Deploy(output) => &output.messages_sent,
            TransactionOutput::DeployAccount(output) => &output.messages_sent,
            TransactionOutput::Invoke(output) => &output.messages_sent,
            TransactionOutput::L1Handler(output) => &output.messages_sent,
        }
    }
}

/// A declare V0 or V1 transaction (same schema but different version).
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeclareTransactionV0V1 {
    pub max_fee: Fee,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub class_hash: ClassHash,
    pub sender_address: ContractAddress,
}

impl TransactionHasher for DeclareTransactionV0V1 {
    fn calculate_transaction_hash(
        &self,
        chain_id: &ChainId,
        transaction_version: &TransactionVersion,
    ) -> Result<TransactionHash, StarknetApiError> {
        if *transaction_version == TransactionVersion::ZERO {
            return get_declare_transaction_v0_hash(self, chain_id, transaction_version);
        }
        if *transaction_version == TransactionVersion::ONE {
            return get_declare_transaction_v1_hash(self, chain_id, transaction_version);
        }
        panic!("Illegal transaction version.");
    }
}

/// A declare V2 transaction.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeclareTransactionV2 {
    pub max_fee: Fee,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub class_hash: ClassHash,
    pub compiled_class_hash: CompiledClassHash,
    pub sender_address: ContractAddress,
}

impl TransactionHasher for DeclareTransactionV2 {
    fn calculate_transaction_hash(
        &self,
        chain_id: &ChainId,
        transaction_version: &TransactionVersion,
    ) -> Result<TransactionHash, StarknetApiError> {
        get_declare_transaction_v2_hash(self, chain_id, transaction_version)
    }
}

/// A declare V3 transaction.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct DeclareTransactionV3 {
    pub resource_bounds: ValidResourceBounds,
    pub tip: Tip,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub class_hash: ClassHash,
    pub compiled_class_hash: CompiledClassHash,
    pub sender_address: ContractAddress,
    pub nonce_data_availability_mode: DataAvailabilityMode,
    pub fee_data_availability_mode: DataAvailabilityMode,
    pub paymaster_data: PaymasterData,
    pub account_deployment_data: AccountDeploymentData,
}

impl TransactionHasher for DeclareTransactionV3 {
    fn calculate_transaction_hash(
        &self,
        chain_id: &ChainId,
        transaction_version: &TransactionVersion,
    ) -> Result<TransactionHash, StarknetApiError> {
        get_declare_transaction_v3_hash(self, chain_id, transaction_version)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum DeclareTransaction {
    V0(DeclareTransactionV0V1),
    V1(DeclareTransactionV0V1),
    V2(DeclareTransactionV2),
    V3(DeclareTransactionV3),
}

macro_rules! implement_declare_tx_getters {
    ($(($field:ident, $field_type:ty)),*) => {
        $(pub fn $field(&self) -> $field_type {
            match self {
                Self::V0(tx) => tx.$field.clone(),
                Self::V1(tx) => tx.$field.clone(),
                Self::V2(tx) => tx.$field.clone(),
                Self::V3(tx) => tx.$field.clone(),
            }
        })*
    };
}

impl DeclareTransaction {
    implement_declare_tx_getters!(
        (class_hash, ClassHash),
        (nonce, Nonce),
        (sender_address, ContractAddress),
        (signature, TransactionSignature)
    );

    implement_v3_tx_getters!(
        (resource_bounds, ValidResourceBounds),
        (tip, Tip),
        (nonce_data_availability_mode, DataAvailabilityMode),
        (fee_data_availability_mode, DataAvailabilityMode),
        (paymaster_data, PaymasterData),
        (account_deployment_data, AccountDeploymentData)
    );

    pub fn compiled_class_hash(&self) -> CompiledClassHash {
        match self {
            DeclareTransaction::V0(_) | DeclareTransaction::V1(_) => {
                panic!("Cairo0 DeclareTransaction (V0, V1) doesn't have compiled class hash.")
            }
            DeclareTransaction::V2(tx) => tx.compiled_class_hash,
            DeclareTransaction::V3(tx) => tx.compiled_class_hash,
        }
    }

    pub fn version(&self) -> TransactionVersion {
        match self {
            DeclareTransaction::V0(_) => TransactionVersion::ZERO,
            DeclareTransaction::V1(_) => TransactionVersion::ONE,
            DeclareTransaction::V2(_) => TransactionVersion::TWO,
            DeclareTransaction::V3(_) => TransactionVersion::THREE,
        }
    }
}

impl TransactionHasher for DeclareTransaction {
    fn calculate_transaction_hash(
        &self,
        chain_id: &ChainId,
        transaction_version: &TransactionVersion,
    ) -> Result<TransactionHash, StarknetApiError> {
        match self {
            DeclareTransaction::V0(tx) => {
                tx.calculate_transaction_hash(chain_id, transaction_version)
            }
            DeclareTransaction::V1(tx) => {
                tx.calculate_transaction_hash(chain_id, transaction_version)
            }
            DeclareTransaction::V2(tx) => {
                tx.calculate_transaction_hash(chain_id, transaction_version)
            }
            DeclareTransaction::V3(tx) => {
                tx.calculate_transaction_hash(chain_id, transaction_version)
            }
        }
    }
}

/// A deploy account V1 transaction.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeployAccountTransactionV1 {
    pub max_fee: Fee,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub class_hash: ClassHash,
    pub contract_address_salt: ContractAddressSalt,
    pub constructor_calldata: Calldata,
}

impl TransactionHasher for DeployAccountTransactionV1 {
    fn calculate_transaction_hash(
        &self,
        chain_id: &ChainId,
        transaction_version: &TransactionVersion,
    ) -> Result<TransactionHash, StarknetApiError> {
        get_deploy_account_transaction_v1_hash(self, chain_id, transaction_version)
    }
}

/// A deploy account V3 transaction.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeployAccountTransactionV3 {
    pub resource_bounds: ValidResourceBounds,
    pub tip: Tip,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub class_hash: ClassHash,
    pub contract_address_salt: ContractAddressSalt,
    pub constructor_calldata: Calldata,
    pub nonce_data_availability_mode: DataAvailabilityMode,
    pub fee_data_availability_mode: DataAvailabilityMode,
    pub paymaster_data: PaymasterData,
}

impl TransactionHasher for DeployAccountTransactionV3 {
    fn calculate_transaction_hash(
        &self,
        chain_id: &ChainId,
        transaction_version: &TransactionVersion,
    ) -> Result<TransactionHash, StarknetApiError> {
        get_deploy_account_transaction_v3_hash(self, chain_id, transaction_version)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord, From)]
pub enum DeployAccountTransaction {
    V1(DeployAccountTransactionV1),
    V3(DeployAccountTransactionV3),
}

macro_rules! implement_deploy_account_tx_getters {
    ($(($field:ident, $field_type:ty)),*) => {
        $(
            pub fn $field(&self) -> $field_type {
                match self {
                    Self::V1(tx) => tx.$field.clone(),
                    Self::V3(tx) => tx.$field.clone(),
                }
            }
        )*
    };
}

impl DeployAccountTransaction {
    implement_deploy_account_tx_getters!(
        (class_hash, ClassHash),
        (constructor_calldata, Calldata),
        (contract_address_salt, ContractAddressSalt),
        (nonce, Nonce),
        (signature, TransactionSignature)
    );

    implement_v3_tx_getters!(
        (resource_bounds, ValidResourceBounds),
        (tip, Tip),
        (nonce_data_availability_mode, DataAvailabilityMode),
        (fee_data_availability_mode, DataAvailabilityMode),
        (paymaster_data, PaymasterData)
    );

    pub fn version(&self) -> TransactionVersion {
        match self {
            DeployAccountTransaction::V1(_) => TransactionVersion::ONE,
            DeployAccountTransaction::V3(_) => TransactionVersion::THREE,
        }
    }
}

impl TransactionHasher for DeployAccountTransaction {
    fn calculate_transaction_hash(
        &self,
        chain_id: &ChainId,
        transaction_version: &TransactionVersion,
    ) -> Result<TransactionHash, StarknetApiError> {
        match self {
            DeployAccountTransaction::V1(tx) => {
                tx.calculate_transaction_hash(chain_id, transaction_version)
            }
            DeployAccountTransaction::V3(tx) => {
                tx.calculate_transaction_hash(chain_id, transaction_version)
            }
        }
    }
}

/// A deploy transaction.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct DeployTransaction {
    pub version: TransactionVersion,
    pub class_hash: ClassHash,
    pub contract_address_salt: ContractAddressSalt,
    pub constructor_calldata: Calldata,
}

impl TransactionHasher for DeployTransaction {
    fn calculate_transaction_hash(
        &self,
        chain_id: &ChainId,
        transaction_version: &TransactionVersion,
    ) -> Result<TransactionHash, StarknetApiError> {
        get_deploy_transaction_hash(self, chain_id, transaction_version)
    }
}

/// An invoke V0 transaction.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct InvokeTransactionV0 {
    pub max_fee: Fee,
    pub signature: TransactionSignature,
    pub contract_address: ContractAddress,
    pub entry_point_selector: EntryPointSelector,
    pub calldata: Calldata,
}

impl TransactionHasher for InvokeTransactionV0 {
    fn calculate_transaction_hash(
        &self,
        chain_id: &ChainId,
        transaction_version: &TransactionVersion,
    ) -> Result<TransactionHash, StarknetApiError> {
        get_invoke_transaction_v0_hash(self, chain_id, transaction_version)
    }
}

/// An invoke V1 transaction.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct InvokeTransactionV1 {
    pub max_fee: Fee,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub sender_address: ContractAddress,
    pub calldata: Calldata,
}

impl TransactionHasher for InvokeTransactionV1 {
    fn calculate_transaction_hash(
        &self,
        chain_id: &ChainId,
        transaction_version: &TransactionVersion,
    ) -> Result<TransactionHash, StarknetApiError> {
        get_invoke_transaction_v1_hash(self, chain_id, transaction_version)
    }
}

/// An invoke V3 transaction.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct InvokeTransactionV3 {
    pub resource_bounds: ValidResourceBounds,
    pub tip: Tip,
    pub signature: TransactionSignature,
    pub nonce: Nonce,
    pub sender_address: ContractAddress,
    pub calldata: Calldata,
    pub nonce_data_availability_mode: DataAvailabilityMode,
    pub fee_data_availability_mode: DataAvailabilityMode,
    pub paymaster_data: PaymasterData,
    pub account_deployment_data: AccountDeploymentData,
}

impl TransactionHasher for InvokeTransactionV3 {
    fn calculate_transaction_hash(
        &self,
        chain_id: &ChainId,
        transaction_version: &TransactionVersion,
    ) -> Result<TransactionHash, StarknetApiError> {
        get_invoke_transaction_v3_hash(self, chain_id, transaction_version)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord, From)]
pub enum InvokeTransaction {
    V0(InvokeTransactionV0),
    V1(InvokeTransactionV1),
    V3(InvokeTransactionV3),
}

macro_rules! implement_invoke_tx_getters {
    ($(($field:ident, $field_type:ty)),*) => {
        $(pub fn $field(&self) -> $field_type {
            match self {
                Self::V0(tx) => tx.$field.clone(),
                Self::V1(tx) => tx.$field.clone(),
                Self::V3(tx) => tx.$field.clone(),
            }
        })*
    };
}

impl InvokeTransaction {
    implement_invoke_tx_getters!((calldata, Calldata), (signature, TransactionSignature));

    implement_v3_tx_getters!(
        (resource_bounds, ValidResourceBounds),
        (tip, Tip),
        (nonce_data_availability_mode, DataAvailabilityMode),
        (fee_data_availability_mode, DataAvailabilityMode),
        (paymaster_data, PaymasterData),
        (account_deployment_data, AccountDeploymentData)
    );

    pub fn nonce(&self) -> Nonce {
        match self {
            Self::V0(_) => Nonce::default(),
            Self::V1(tx) => tx.nonce,
            Self::V3(tx) => tx.nonce,
        }
    }

    pub fn sender_address(&self) -> ContractAddress {
        match self {
            Self::V0(tx) => tx.contract_address,
            Self::V1(tx) => tx.sender_address,
            Self::V3(tx) => tx.sender_address,
        }
    }

    pub fn version(&self) -> TransactionVersion {
        match self {
            InvokeTransaction::V0(_) => TransactionVersion::ZERO,
            InvokeTransaction::V1(_) => TransactionVersion::ONE,
            InvokeTransaction::V3(_) => TransactionVersion::THREE,
        }
    }
}

impl TransactionHasher for InvokeTransaction {
    fn calculate_transaction_hash(
        &self,
        chain_id: &ChainId,
        transaction_version: &TransactionVersion,
    ) -> Result<TransactionHash, StarknetApiError> {
        match self {
            InvokeTransaction::V0(tx) => {
                tx.calculate_transaction_hash(chain_id, transaction_version)
            }
            InvokeTransaction::V1(tx) => {
                tx.calculate_transaction_hash(chain_id, transaction_version)
            }
            InvokeTransaction::V3(tx) => {
                tx.calculate_transaction_hash(chain_id, transaction_version)
            }
        }
    }
}

/// An L1 handler transaction.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct L1HandlerTransaction {
    pub version: TransactionVersion,
    pub nonce: Nonce,
    pub contract_address: ContractAddress,
    pub entry_point_selector: EntryPointSelector,
    pub calldata: Calldata,
}

impl TransactionHasher for L1HandlerTransaction {
    fn calculate_transaction_hash(
        &self,
        chain_id: &ChainId,
        transaction_version: &TransactionVersion,
    ) -> Result<TransactionHash, StarknetApiError> {
        get_l1_handler_transaction_hash(self, chain_id, transaction_version)
    }
}

/// A declare transaction output.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct DeclareTransactionOutput {
    pub actual_fee: Fee,
    pub messages_sent: Vec<MessageToL1>,
    pub events: Vec<Event>,
    #[serde(flatten)]
    pub execution_status: TransactionExecutionStatus,
    pub execution_resources: ExecutionResources,
}

/// A deploy-account transaction output.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct DeployAccountTransactionOutput {
    pub actual_fee: Fee,
    pub messages_sent: Vec<MessageToL1>,
    pub events: Vec<Event>,
    pub contract_address: ContractAddress,
    #[serde(flatten)]
    pub execution_status: TransactionExecutionStatus,
    pub execution_resources: ExecutionResources,
}

/// A deploy transaction output.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct DeployTransactionOutput {
    pub actual_fee: Fee,
    pub messages_sent: Vec<MessageToL1>,
    pub events: Vec<Event>,
    pub contract_address: ContractAddress,
    #[serde(flatten)]
    pub execution_status: TransactionExecutionStatus,
    pub execution_resources: ExecutionResources,
}

/// An invoke transaction output.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct InvokeTransactionOutput {
    pub actual_fee: Fee,
    pub messages_sent: Vec<MessageToL1>,
    pub events: Vec<Event>,
    #[serde(flatten)]
    pub execution_status: TransactionExecutionStatus,
    pub execution_resources: ExecutionResources,
}

/// An L1 handler transaction output.
#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct L1HandlerTransactionOutput {
    pub actual_fee: Fee,
    pub messages_sent: Vec<MessageToL1>,
    pub events: Vec<Event>,
    #[serde(flatten)]
    pub execution_status: TransactionExecutionStatus,
    pub execution_resources: ExecutionResources,
}

/// A transaction receipt.
#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct TransactionReceipt {
    pub transaction_hash: TransactionHash,
    pub block_hash: BlockHash,
    pub block_number: BlockNumber,
    #[serde(flatten)]
    pub output: TransactionOutput,
}

/// Transaction execution status.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord, Default)]
#[serde(tag = "execution_status")]
pub enum TransactionExecutionStatus {
    #[serde(rename = "SUCCEEDED")]
    #[default]
    // Succeeded is the default variant because old versions of Starknet don't have an execution
    // status and every transaction is considered succeeded
    Succeeded,
    #[serde(rename = "REVERTED")]
    Reverted(RevertedTransactionExecutionStatus),
}

/// A reverted transaction execution status.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct RevertedTransactionExecutionStatus {
    // TODO: Validate it's an ASCII string.
    pub revert_reason: String,
}

/// A fee.
#[cfg_attr(any(test, feature = "testing"), derive(derive_more::Add, derive_more::Deref))]
#[derive(
    Debug,
    Copy,
    Clone,
    Default,
    Display,
    Eq,
    PartialEq,
    Hash,
    Deserialize,
    Serialize,
    PartialOrd,
    Ord,
)]
#[serde(from = "PrefixedBytesAsHex<16_usize>", into = "PrefixedBytesAsHex<16_usize>")]
pub struct Fee(pub u128);

impl Fee {
    pub fn checked_add(self, rhs: Fee) -> Option<Fee> {
        self.0.checked_add(rhs.0).map(Fee)
    }

    pub fn saturating_add(self, rhs: Self) -> Self {
        Self(self.0.saturating_add(rhs.0))
    }

    pub fn checked_div_ceil(self, rhs: NonzeroGasPrice) -> Option<GasAmount> {
        self.checked_div(rhs).map(|value| {
            if value
                .checked_mul(rhs.into())
                .expect("Multiplying by denominator of floor division cannot overflow.")
                < self
            {
                (value.0 + 1).into()
            } else {
                value
            }
        })
    }

    pub fn checked_div(self, rhs: NonzeroGasPrice) -> Option<GasAmount> {
        match u64::try_from(self.0 / rhs.get().0) {
            Ok(value) => Some(value.into()),
            Err(_) => None,
        }
    }

    pub fn saturating_div(self, rhs: NonzeroGasPrice) -> GasAmount {
        self.checked_div(rhs).unwrap_or(GasAmount::MAX)
    }
}

impl From<PrefixedBytesAsHex<16_usize>> for Fee {
    fn from(value: PrefixedBytesAsHex<16_usize>) -> Self {
        Self(u128::from_be_bytes(value.0))
    }
}

impl From<Fee> for PrefixedBytesAsHex<16_usize> {
    fn from(fee: Fee) -> Self {
        Self(fee.0.to_be_bytes())
    }
}

impl From<Fee> for Felt {
    fn from(fee: Fee) -> Self {
        Self::from(fee.0)
    }
}

/// The hash of a [Transaction](`crate::transaction::Transaction`).
#[derive(
    Debug,
    Default,
    Copy,
    Clone,
    Eq,
    PartialEq,
    Hash,
    Deserialize,
    Serialize,
    PartialOrd,
    Ord,
    derive_more::Deref,
)]
pub struct TransactionHash(pub StarkHash);

impl Display for TransactionHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A contract address salt.
#[derive(
    Debug, Copy, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct ContractAddressSalt(pub StarkHash);

/// A transaction signature.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct TransactionSignature(pub Vec<Felt>);

/// A transaction version.
#[derive(
    Debug,
    Copy,
    Clone,
    Default,
    Eq,
    PartialEq,
    Hash,
    Deserialize,
    Serialize,
    PartialOrd,
    Ord,
    derive_more::Deref,
)]
pub struct TransactionVersion(pub Felt);

impl TransactionVersion {
    /// [TransactionVersion] constant that's equal to 0.
    pub const ZERO: Self = { Self(Felt::ZERO) };

    /// [TransactionVersion] constant that's equal to 1.
    pub const ONE: Self = { Self(Felt::ONE) };

    /// [TransactionVersion] constant that's equal to 2.
    pub const TWO: Self = { Self(Felt::TWO) };

    /// [TransactionVersion] constant that's equal to 3.
    pub const THREE: Self = { Self(Felt::THREE) };
}

// TODO: TransactionVersion and SignedTransactionVersion should probably be separate types.
// Returns the transaction version taking into account the transaction options.
pub fn signed_tx_version_from_tx(
    tx: &Transaction,
    transaction_options: &TransactionOptions,
) -> TransactionVersion {
    signed_tx_version(&tx.version(), transaction_options)
}

pub fn signed_tx_version(
    tx_version: &TransactionVersion,
    transaction_options: &TransactionOptions,
) -> TransactionVersion {
    // If only_query is true, set the 128-th bit.
    let query_only_bit = Felt::TWO.pow(QUERY_VERSION_BASE_BIT);
    assert_eq!(
        tx_version.0.to_biguint() & query_only_bit.to_biguint(),
        BigUint::from(0_u8),
        "Requested signed tx version with version that already has query bit set: {tx_version:?}."
    );
    if transaction_options.only_query {
        TransactionVersion(tx_version.0 + query_only_bit)
    } else {
        *tx_version
    }
}

/// The calldata of a transaction.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct Calldata(pub Arc<Vec<Felt>>);

#[macro_export]
macro_rules! calldata {
    ( $( $x:expr ),* ) => {
        Calldata(vec![$($x),*].into())
    };
}

/// An L1 to L2 message.
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct MessageToL2 {
    pub from_address: EthAddress,
    pub payload: L1ToL2Payload,
}

/// An L2 to L1 message.
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct MessageToL1 {
    pub from_address: ContractAddress,
    pub to_address: EthAddress,
    pub payload: L2ToL1Payload,
}

/// The payload of [`MessageToL2`].
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct L1ToL2Payload(pub Vec<Felt>);

/// The payload of [`MessageToL1`].
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct L2ToL1Payload(pub Vec<Felt>);

/// An event.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct Event {
    // TODO: Add a TransactionHash element to this struct, and then remove EventLeafElements.
    pub from_address: ContractAddress,
    #[serde(flatten)]
    pub content: EventContent,
}

/// An event content.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct EventContent {
    pub keys: Vec<EventKey>,
    pub data: EventData,
}

/// An event key.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct EventKey(pub Felt);

/// An event data.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct EventData(pub Vec<Felt>);

/// The index of a transaction in [BlockBody](`crate::block::BlockBody`).
#[derive(
    Debug, Default, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct TransactionOffsetInBlock(pub usize);

/// The index of an event in [TransactionOutput](`crate::transaction::TransactionOutput`).
#[derive(
    Debug, Default, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct EventIndexInTransactionOutput(pub usize);

/// Transaction fee tip.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Deserialize,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Serialize,
    derive_more::Deref,
)]
#[serde(from = "PrefixedBytesAsHex<8_usize>", into = "PrefixedBytesAsHex<8_usize>")]
pub struct Tip(pub u64);

impl From<PrefixedBytesAsHex<8_usize>> for Tip {
    fn from(value: PrefixedBytesAsHex<8_usize>) -> Self {
        Self(u64::from_be_bytes(value.0))
    }
}

impl From<Tip> for PrefixedBytesAsHex<8_usize> {
    fn from(tip: Tip) -> Self {
        Self(tip.0.to_be_bytes())
    }
}

impl From<Tip> for Felt {
    fn from(tip: Tip) -> Self {
        Self::from(tip.0)
    }
}

/// Execution resource.
#[derive(
    Clone,
    Copy,
    Debug,
    Deserialize,
    Display,
    EnumIter,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Serialize,
)]
pub enum Resource {
    #[serde(rename = "L1_GAS")]
    L1Gas,
    #[serde(rename = "L2_GAS")]
    L2Gas,
    #[serde(rename = "L1_DATA")]
    L1DataGas,
}

impl Resource {
    pub fn to_hex(&self) -> &'static str {
        match self {
            Resource::L1Gas => "0x00000000000000000000000000000000000000000000000000004c315f474153",
            Resource::L2Gas => "0x00000000000000000000000000000000000000000000000000004c325f474153",
            Resource::L1DataGas => {
                "0x000000000000000000000000000000000000000000000000004c315f44415441"
            }
        }
    }
}

/// Fee bounds for an execution resource.
/// TODO(Yael): add types ResourceAmount and ResourcePrice and use them instead of u64 and u128.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
// TODO(Nimrod): Consider renaming this struct.
pub struct ResourceBounds {
    // Specifies the maximum amount of each resource allowed for usage during the execution.
    #[serde(serialize_with = "gas_amount_to_hex", deserialize_with = "hex_to_gas_amount")]
    pub max_amount: GasAmount,

    // Specifies the maximum price the user is willing to pay for each resource unit.
    #[serde(serialize_with = "gas_price_to_hex", deserialize_with = "hex_to_gas_price")]
    pub max_price_per_unit: GasPrice,
}

impl ResourceBounds {
    /// Returns true iff both the max amount and the max amount per unit is zero.
    pub fn is_zero(&self) -> bool {
        self.max_amount == GasAmount(0) && self.max_price_per_unit == GasPrice(0)
    }
}

fn gas_amount_to_hex<S>(value: &GasAmount, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&format!("0x{:x}", value.0))
}

fn hex_to_gas_amount<'de, D>(deserializer: D) -> Result<GasAmount, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    Ok(GasAmount(
        u64::from_str_radix(s.trim_start_matches("0x"), 16).map_err(serde::de::Error::custom)?,
    ))
}

fn gas_price_to_hex<S>(value: &GasPrice, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&format!("0x{:x}", value.0))
}

fn hex_to_gas_price<'de, D>(deserializer: D) -> Result<GasPrice, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    Ok(GasPrice(
        u128::from_str_radix(s.trim_start_matches("0x"), 16).map_err(serde::de::Error::custom)?,
    ))
}

#[derive(Debug, PartialEq)]
pub enum GasVectorComputationMode {
    All,
    NoL2Gas,
}

#[derive(Debug, PartialEq)]
pub enum GasVectorComputationMode {
    All,
    NoL2Gas,
}

/// A mapping from execution resources to their corresponding fee bounds..
#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
// TODO(Nimrod): Remove this struct definition.
pub struct DeprecatedResourceBoundsMapping(pub BTreeMap<Resource, ResourceBounds>);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum ValidResourceBounds {
    L1Gas(ResourceBounds), // Pre 0.13.3. Only L1 gas. L2 bounds are signed but never used.
    AllResources(AllResourceBounds),
}

impl ValidResourceBounds {
    pub fn get_l1_bounds(&self) -> ResourceBounds {
        match self {
            Self::L1Gas(l1_bounds) => *l1_bounds,
            Self::AllResources(AllResourceBounds { l1_gas, .. }) => *l1_gas,
        }
    }

    pub fn get_l2_bounds(&self) -> ResourceBounds {
        match self {
            Self::L1Gas(_) => ResourceBounds::default(),
            Self::AllResources(AllResourceBounds { l2_gas, .. }) => *l2_gas,
        }
    }

    /// Returns the maximum possible fee that can be charged for the transaction.
    /// The computation is saturating, meaning that if the result is larger than the maximum
    /// possible fee, the maximum possible fee is returned.
    pub fn max_possible_fee(&self) -> Fee {
        match self {
            ValidResourceBounds::L1Gas(l1_bounds) => {
                l1_bounds.max_amount.saturating_mul(l1_bounds.max_price_per_unit)
            }
            ValidResourceBounds::AllResources(AllResourceBounds {
                l1_gas,
                l2_gas,
                l1_data_gas,
            }) => l1_gas
                .max_amount
                .saturating_mul(l1_gas.max_price_per_unit)
                .saturating_add(l2_gas.max_amount.saturating_mul(l2_gas.max_price_per_unit))
                .saturating_add(
                    l1_data_gas.max_amount.saturating_mul(l1_data_gas.max_price_per_unit),
                ),
        }
    }

    pub fn get_gas_vector_computation_mode(&self) -> GasVectorComputationMode {
        match self {
            Self::AllResources(_) => GasVectorComputationMode::All,
            Self::L1Gas(_) => GasVectorComputationMode::NoL2Gas,
        }
    }

    // TODO(Nimrod): Default testing bounds should probably be AllResourceBounds variant.
    #[cfg(any(feature = "testing", test))]
    pub fn create_for_testing() -> Self {
        Self::L1Gas(ResourceBounds { max_amount: GasAmount(0), max_price_per_unit: GasPrice(1) })
    }
}

#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize,
)]
pub struct AllResourceBounds {
    pub l1_gas: ResourceBounds,
    pub l2_gas: ResourceBounds,
    pub l1_data_gas: ResourceBounds,
}

impl AllResourceBounds {
    pub fn get_bound(&self, resource: Resource) -> ResourceBounds {
        match resource {
            Resource::L1Gas => self.l1_gas,
            Resource::L2Gas => self.l2_gas,
            Resource::L1DataGas => self.l1_data_gas,
        }
    }
}

/// Deserializes raw resource bounds, given as map, into valid resource bounds.
// TODO(Nimrod): Figure out how to get same result with adding #[derive(Deserialize)].
impl<'de> Deserialize<'de> for ValidResourceBounds {
    fn deserialize<D>(de: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw_resource_bounds: BTreeMap<Resource, ResourceBounds> = Deserialize::deserialize(de)?;
        ValidResourceBounds::try_from(DeprecatedResourceBoundsMapping(raw_resource_bounds))
            .map_err(serde::de::Error::custom)
    }
}

/// Serializes ValidResourceBounds as map for Backwards compatibility.
// TODO(Nimrod): Figure out how to get same result with adding #[derive(Serialize)].
impl Serialize for ValidResourceBounds {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let map = match self {
            ValidResourceBounds::L1Gas(l1_gas) => BTreeMap::from([
                (Resource::L1Gas, *l1_gas),
                (Resource::L2Gas, ResourceBounds::default()),
            ]),
            ValidResourceBounds::AllResources(AllResourceBounds {
                l1_gas,
                l2_gas,
                l1_data_gas,
            }) => BTreeMap::from([
                (Resource::L1Gas, *l1_gas),
                (Resource::L2Gas, *l2_gas),
                (Resource::L1DataGas, *l1_data_gas),
            ]),
        };
        DeprecatedResourceBoundsMapping(map).serialize(s)
    }
}

impl TryFrom<DeprecatedResourceBoundsMapping> for ValidResourceBounds {
    type Error = StarknetApiError;
    fn try_from(
        resource_bounds_mapping: DeprecatedResourceBoundsMapping,
    ) -> Result<Self, Self::Error> {
        if let (Some(l1_bounds), Some(l2_bounds)) = (
            resource_bounds_mapping.0.get(&Resource::L1Gas),
            resource_bounds_mapping.0.get(&Resource::L2Gas),
        ) {
            match resource_bounds_mapping.0.get(&Resource::L1DataGas) {
                Some(data_bounds) => Ok(Self::AllResources(AllResourceBounds {
                    l1_gas: *l1_bounds,
                    l1_data_gas: *data_bounds,
                    l2_gas: *l2_bounds,
                })),
                None => {
                    if l2_bounds.is_zero() {
                        Ok(Self::L1Gas(*l1_bounds))
                    } else {
                        Err(StarknetApiError::InvalidResourceMappingInitializer(format!(
                            "Missing data gas bounds but L2 gas bound is not zero: \
                             {resource_bounds_mapping:?}",
                        )))
                    }
                }
            }
        } else {
            Err(StarknetApiError::InvalidResourceMappingInitializer(format!(
                "{resource_bounds_mapping:?}",
            )))
        }
    }
}

/// Paymaster-related data.
#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct PaymasterData(pub Vec<Felt>);

/// If nonempty, will contain the required data for deploying and initializing an account contract:
/// its class hash, address salt and constructor calldata.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct AccountDeploymentData(pub Vec<Felt>);
