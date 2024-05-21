use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Display;
use std::sync::Arc;

use derive_more::From;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

use crate::block::{BlockHash, BlockNumber};
use crate::core::{
    ChainId, ClassHash, CompiledClassHash, ContractAddress, EntryPointSelector, EthAddress, Nonce,
};
use crate::data_availability::DataAvailabilityMode;
use crate::hash::{StarkFelt, StarkHash};
use crate::serde_utils::PrefixedBytesAsHex;
use crate::transaction_hash::{
    get_declare_transaction_v0_hash, get_declare_transaction_v1_hash,
    get_declare_transaction_v2_hash, get_declare_transaction_v3_hash,
    get_deploy_account_transaction_v1_hash, get_deploy_account_transaction_v3_hash,
    get_deploy_transaction_hash, get_invoke_transaction_v0_hash, get_invoke_transaction_v1_hash,
    get_invoke_transaction_v3_hash, get_l1_handler_transaction_hash,
};
use crate::StarknetApiError;

pub trait TransactionHasher {
    fn calculate_transaction_hash(
        &self,
        chain_id: &ChainId,
        transaction_version: &TransactionVersion,
    ) -> Result<TransactionHash, StarknetApiError>;
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
}

impl TransactionHasher for Transaction {
    fn calculate_transaction_hash(
        &self,
        chain_id: &ChainId,
        transaction_version: &TransactionVersion,
    ) -> Result<TransactionHash, StarknetApiError> {
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

    pub fn execution_resources(&self) -> &ExecutionResources {
        match self {
            TransactionOutput::Declare(output) => &output.execution_resources,
            TransactionOutput::Deploy(output) => &output.execution_resources,
            TransactionOutput::DeployAccount(output) => &output.execution_resources,
            TransactionOutput::Invoke(output) => &output.execution_resources,
            TransactionOutput::L1Handler(output) => &output.execution_resources,
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
        match *transaction_version {
            TransactionVersion::ZERO => {
                get_declare_transaction_v0_hash(self, chain_id, transaction_version)
            }
            TransactionVersion::ONE => {
                get_declare_transaction_v1_hash(self, chain_id, transaction_version)
            }
            _ => panic!("Illegal transaction version."),
        }
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
    pub resource_bounds: ResourceBoundsMapping,
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
    pub resource_bounds: ResourceBoundsMapping,
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
    pub resource_bounds: ResourceBoundsMapping,
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
    pub revert_reason: String,
}

/// A fee.
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
#[serde(from = "PrefixedBytesAsHex<16_usize>", into = "PrefixedBytesAsHex<16_usize>")]
pub struct Fee(pub u128);

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

impl From<Fee> for StarkFelt {
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
pub struct TransactionSignature(pub Vec<StarkFelt>);

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
pub struct TransactionVersion(pub StarkFelt);

impl TransactionVersion {
    /// [TransactionVersion] constant that's equal to 0.
    pub const ZERO: Self = { Self(StarkFelt::ZERO) };

    /// [TransactionVersion] constant that's equal to 1.
    pub const ONE: Self = { Self(StarkFelt::ONE) };

    /// [TransactionVersion] constant that's equal to 2.
    pub const TWO: Self = { Self(StarkFelt::TWO) };

    /// [TransactionVersion] constant that's equal to 3.
    pub const THREE: Self = { Self(StarkFelt::THREE) };
}

/// The calldata of a transaction.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct Calldata(pub Arc<Vec<StarkFelt>>);

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
pub struct L1ToL2Payload(pub Vec<StarkFelt>);

/// The payload of [`MessageToL1`].
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct L2ToL1Payload(pub Vec<StarkFelt>);

/// An event.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct Event {
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
pub struct EventKey(pub StarkFelt);

/// An event data.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct EventData(pub Vec<StarkFelt>);

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

impl From<Tip> for StarkFelt {
    fn from(tip: Tip) -> Self {
        Self::from(tip.0)
    }
}

/// Execution resource.
#[derive(
    Clone, Copy, Debug, Deserialize, EnumIter, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub enum Resource {
    #[serde(rename = "L1_GAS")]
    L1Gas,
    #[serde(rename = "L2_GAS")]
    L2Gas,
}

/// Fee bounds for an execution resource.
/// TODO(Yael): add types ResourceAmount and ResourcePrice and use them instead of u64 and u128.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct ResourceBounds {
    // Specifies the maximum amount of each resource allowed for usage during the execution.
    #[serde(serialize_with = "u64_to_hex", deserialize_with = "hex_to_u64")]
    pub max_amount: u64,

    // Specifies the maximum price the user is willing to pay for each resource unit.
    #[serde(serialize_with = "u128_to_hex", deserialize_with = "hex_to_u128")]
    pub max_price_per_unit: u128,
}

fn u64_to_hex<S>(value: &u64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&format!("0x{:x}", value))
}

fn hex_to_u64<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    u64::from_str_radix(s.trim_start_matches("0x"), 16).map_err(serde::de::Error::custom)
}

fn u128_to_hex<S>(value: &u128, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&format!("0x{:x}", value))
}

fn hex_to_u128<'de, D>(deserializer: D) -> Result<u128, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    u128::from_str_radix(s.trim_start_matches("0x"), 16).map_err(serde::de::Error::custom)
}

/// A mapping from execution resources to their corresponding fee bounds..
#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ResourceBoundsMapping(pub BTreeMap<Resource, ResourceBounds>);

impl TryFrom<Vec<(Resource, ResourceBounds)>> for ResourceBoundsMapping {
    type Error = StarknetApiError;
    fn try_from(
        resource_resource_bounds_pairs: Vec<(Resource, ResourceBounds)>,
    ) -> Result<Self, Self::Error> {
        let n_variants = Resource::iter().count();
        let unique_resources: HashSet<Resource> =
            HashSet::from_iter(resource_resource_bounds_pairs.iter().map(|(k, _)| *k));
        if unique_resources.len() != n_variants
            || resource_resource_bounds_pairs.len() != n_variants
        {
            Err(StarknetApiError::InvalidResourceMappingInitializer(format!(
                "{:?}",
                resource_resource_bounds_pairs
            )))
        } else {
            Ok(Self(resource_resource_bounds_pairs.into_iter().collect::<BTreeMap<_, _>>()))
        }
    }
}

/// Paymaster-related data.
#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct PaymasterData(pub Vec<StarkFelt>);

/// If nonempty, will contain the required data for deploying and initializing an account contract:
/// its class hash, address salt and constructor calldata.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct AccountDeploymentData(pub Vec<StarkFelt>);

/// The execution resources used by a transaction.
#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct ExecutionResources {
    pub steps: u64,
    pub builtin_instance_counter: HashMap<Builtin, u64>,
    pub memory_holes: u64,
    pub da_l1_gas_consumed: u64,
    pub da_l1_data_gas_consumed: u64,
}

#[derive(Hash, Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub enum Builtin {
    #[serde(rename = "range_check_builtin_applications")]
    RangeCheck,
    #[serde(rename = "pedersen_builtin_applications")]
    Pedersen,
    #[serde(rename = "poseidon_builtin_applications")]
    Poseidon,
    #[serde(rename = "ec_op_builtin_applications")]
    EcOp,
    #[serde(rename = "ecdsa_builtin_applications")]
    Ecdsa,
    #[serde(rename = "bitwise_builtin_applications")]
    Bitwise,
    #[serde(rename = "keccak_builtin_applications")]
    Keccak,
    #[serde(rename = "segment_arena_builtin")]
    SegmentArena,
}
