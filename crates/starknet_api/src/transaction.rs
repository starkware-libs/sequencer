use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
use starknet_types_core::felt::Felt;

use crate::block::{BlockHash, BlockNumber};
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
use crate::execution_resources::ExecutionResources;
use crate::hash::StarkHash;
use crate::transaction::fields::{
    AccountDeploymentData,
    Calldata,
    ContractAddressSalt,
    Fee,
    PaymasterData,
    Tip,
    TransactionSignature,
    ValidResourceBounds,
};
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

pub mod constants;
pub mod fields;

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

impl From<crate::executable_transaction::Transaction> for Transaction {
    fn from(tx: crate::executable_transaction::Transaction) -> Self {
        match tx {
            crate::executable_transaction::Transaction::L1Handler(_) => {
                unimplemented!("L1Handler transactions are not supported yet.")
            }
            crate::executable_transaction::Transaction::Account(account_tx) => match account_tx {
                crate::executable_transaction::AccountTransaction::Declare(tx) => {
                    Transaction::Declare(tx.tx)
                }
                crate::executable_transaction::AccountTransaction::DeployAccount(tx) => {
                    Transaction::DeployAccount(tx.tx)
                }
                crate::executable_transaction::AccountTransaction::Invoke(tx) => {
                    Transaction::Invoke(tx.tx)
                }
            },
        }
    }
}

impl From<(Transaction, TransactionHash)> for crate::executable_transaction::Transaction {
    fn from((tx, tx_hash): (Transaction, TransactionHash)) -> Self {
        match tx {
            Transaction::Invoke(tx) => crate::executable_transaction::Transaction::Account(
                crate::executable_transaction::AccountTransaction::Invoke(
                    crate::executable_transaction::InvokeTransaction { tx, tx_hash },
                ),
            ),
            _ => {
                unimplemented!("Unsupported transaction type. Only Invoke is currently supported.")
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

#[derive(
    Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord, derive_more::From,
)]
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

#[derive(
    Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord, derive_more::From,
)]
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

impl std::fmt::Display for TransactionHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

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
