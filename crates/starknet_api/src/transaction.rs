use std::sync::LazyLock;

use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
use sizeof::SizeOf;
use starknet_types_core::felt::Felt;

use crate::block::{BlockHash, BlockNumber};
use crate::core::{
    calculate_contract_address,
    ChainId,
    ClassHash,
    CompiledClassHash,
    ContractAddress,
    EntryPointSelector,
    EthAddress,
    Nonce,
    PatriciaKey,
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
use crate::{executable_transaction, StarknetApiError, StarknetApiResult};

#[cfg(test)]
#[path = "transaction_test.rs"]
mod transaction_test;

pub mod constants;
pub mod fields;

pub static QUERY_VERSION_BASE: LazyLock<Felt> = LazyLock::new(|| {
    const QUERY_VERSION_BASE_BIT: u32 = 128;
    Felt::TWO.pow(QUERY_VERSION_BASE_BIT)
});

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

impl From<executable_transaction::Transaction> for Transaction {
    fn from(tx: executable_transaction::Transaction) -> Self {
        match tx {
            executable_transaction::Transaction::L1Handler(tx) => Transaction::L1Handler(tx.tx),
            executable_transaction::Transaction::Account(account_tx) => match account_tx {
                executable_transaction::AccountTransaction::Declare(tx) => {
                    Transaction::Declare(tx.tx)
                }
                executable_transaction::AccountTransaction::DeployAccount(tx) => {
                    Transaction::DeployAccount(tx.tx)
                }
                executable_transaction::AccountTransaction::Invoke(tx) => {
                    Transaction::Invoke(tx.tx)
                }
            },
        }
    }
}

impl TryFrom<(Transaction, &ChainId)> for executable_transaction::Transaction {
    type Error = StarknetApiError;

    fn try_from((tx, chain_id): (Transaction, &ChainId)) -> Result<Self, Self::Error> {
        let tx_hash = tx.calculate_transaction_hash(chain_id)?;
        match tx {
            Transaction::DeployAccount(tx) => {
                let contract_address = tx.calculate_contract_address()?;
                Ok(executable_transaction::Transaction::Account(
                    executable_transaction::AccountTransaction::DeployAccount(
                        executable_transaction::DeployAccountTransaction {
                            tx,
                            tx_hash,
                            contract_address,
                        },
                    ),
                ))
            }
            Transaction::Invoke(tx) => Ok(executable_transaction::Transaction::Account(
                executable_transaction::AccountTransaction::Invoke(
                    executable_transaction::InvokeTransaction { tx, tx_hash },
                ),
            )),
            Transaction::L1Handler(tx) => Ok(executable_transaction::Transaction::L1Handler(
                executable_transaction::L1HandlerTransaction {
                    tx,
                    tx_hash,
                    // TODO(yael): The paid fee should be an input from the l1_handler.
                    paid_fee_on_l1: Fee(1),
                },
            )),
            _ => {
                unimplemented!(
                    "Unsupported transaction type. Only DeployAccount, Invoke and L1Handler are \
                     currently supported. tx: {:?}",
                    tx
                )
            }
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub struct TransactionOptions {
    /// Transaction that shouldn't be broadcasted to StarkNet. For example, users that want to
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
#[cfg_attr(any(test, feature = "testing"), derive(Default))]
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

pub trait CalculateContractAddress {
    fn calculate_contract_address(&self) -> StarknetApiResult<ContractAddress>;
}

/// A trait intended for deploy account transactions. Structs implementing this trait derive the
/// implementation of [CalculateContractAddress].
pub trait DeployTransactionTrait {
    fn contract_address_salt(&self) -> ContractAddressSalt;
    fn class_hash(&self) -> ClassHash;
    fn constructor_calldata(&self) -> &Calldata;
}

#[macro_export]
macro_rules! impl_deploy_transaction_trait {
    ($type:ty) => {
        impl DeployTransactionTrait for $type {
            fn contract_address_salt(&self) -> ContractAddressSalt {
                self.contract_address_salt
            }

            fn class_hash(&self) -> ClassHash {
                self.class_hash
            }

            fn constructor_calldata(&self) -> &Calldata {
                &self.constructor_calldata
            }
        }
    };
}

impl<T: DeployTransactionTrait> CalculateContractAddress for T {
    /// Calculates the contract address for the contract deployed by a deploy account transaction.
    /// For more details see:
    /// <https://docs.starknet.io/architecture-and-concepts/smart-contracts/contract-address/>
    fn calculate_contract_address(&self) -> StarknetApiResult<ContractAddress> {
        // When the contract is deployed via a deploy-account transaction, the deployer address is
        // zero.
        const DEPLOYER_ADDRESS: ContractAddress = ContractAddress(PatriciaKey::ZERO);
        calculate_contract_address(
            self.contract_address_salt(),
            self.class_hash(),
            self.constructor_calldata(),
            DEPLOYER_ADDRESS,
        )
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

impl_deploy_transaction_trait!(DeployAccountTransactionV1);

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

impl_deploy_transaction_trait!(DeployAccountTransactionV3);

#[derive(
    Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord, derive_more::From,
)]
pub enum DeployAccountTransaction {
    V1(DeployAccountTransactionV1),
    V3(DeployAccountTransactionV3),
}

impl CalculateContractAddress for DeployAccountTransaction {
    fn calculate_contract_address(&self) -> StarknetApiResult<ContractAddress> {
        match self {
            DeployAccountTransaction::V1(tx) => tx.calculate_contract_address(),
            DeployAccountTransaction::V3(tx) => tx.calculate_contract_address(),
        }
    }
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
    // TODO(Arni): Consider using a direct reference to the getters from [DeployTrait].
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

// The trait [`DeployTransactionTrait`] is intended for [`DeployAccountTransaction`].
// The calculation of the contract address of the contract deployed by the deprecated
// [`DeployTransaction`] is consistent with that calculation.
impl_deploy_transaction_trait!(DeployTransaction);

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

impl L1HandlerTransaction {
    /// The transaction version is considered 0 for L1-Handler transaction for hash calculation
    /// purposes.
    pub const VERSION: TransactionVersion = TransactionVersion::ZERO;
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
    // TODO(YoavGr): Validate it's an ASCII string.
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
    SizeOf,
)]
pub struct TransactionHash(pub StarkHash);

impl std::fmt::Display for TransactionHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.to_hex_string())
    }
}

// Use this in tests to get a randomly generate transaction hash.
// Note that get_test_instance uses StarkHash::default, not a random value.
#[cfg(any(feature = "testing", test))]
impl TransactionHash {
    pub fn random(rng: &mut impl rand::Rng) -> Self {
        let mut byte_vec = vec![];
        for _ in 0..32 {
            byte_vec.push(rng.gen::<u8>());
        }
        let byte_array = byte_vec.try_into().expect("Expected a Vec of length 32");
        TransactionHash(StarkHash::from_bytes_be(&byte_array))
    }
}

/// A utility macro to create a [`TransactionHash`] from an unsigned integer representation.
#[cfg(any(feature = "testing", test))]
#[macro_export]
macro_rules! tx_hash {
    ($tx_hash:expr) => {
        $crate::transaction::TransactionHash($crate::hash::StarkHash::from($tx_hash))
    };
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

// TODO(Dori): TransactionVersion and SignedTransactionVersion should probably be separate types.
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
    let query_only_bit = *QUERY_VERSION_BASE;
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
    // TODO(Gilad): Add a TransactionHash element to this struct, and then remove
    // EventLeafElements.
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
