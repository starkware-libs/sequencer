#[cfg(test)]
#[path = "executable_transaction_test.rs"]
mod executable_transaction_test;

use std::str::FromStr;

use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use serde::{Deserialize, Serialize};
use starknet_types_core::felt::Felt;
use strum_macros::EnumIter;
use thiserror::Error;

use crate::contract_class::{ClassInfo, ContractClass};
use crate::core::{ChainId, ClassHash, CompiledClassHash, ContractAddress, Nonce};
use crate::data_availability::DataAvailabilityMode;
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
use crate::transaction::{
    CalculateContractAddress,
    TransactionHash,
    TransactionHasher,
    TransactionVersion,
};
use crate::StarknetApiError;

macro_rules! implement_inner_tx_getter_calls {
    ($(($field:ident, $field_type:ty)),*) => {
        $(pub fn $field(&self) -> $field_type {
            self.tx.$field().clone()
        })*
    };
}

macro_rules! implement_getter_calls {
    ($(($field:ident, $field_type:ty)),*) => {
        $(pub fn $field(&self) -> $field_type {
            self.$field
        })*
};
}

macro_rules! implement_account_tx_inner_getters {
    ($(($field:ident, $field_type:ty)),*) => {
        $(pub fn $field(&self) -> $field_type {
            match self {
                AccountTransaction::Declare(tx) => tx.tx.$field().clone(),
                AccountTransaction::DeployAccount(tx) => tx.tx.$field().clone(),
                AccountTransaction::Invoke(tx) => tx.tx.$field().clone(),
            }
        })*
    };
}

#[derive(Clone, Copy, Debug, Deserialize, EnumIter, Eq, Hash, PartialEq, Serialize)]
pub enum TransactionType {
    Declare,
    DeployAccount,
    InvokeFunction,
    L1Handler,
}

impl FromStr for TransactionType {
    type Err = StarknetApiError;

    fn from_str(tx_type: &str) -> Result<Self, Self::Err> {
        match tx_type {
            "Declare" | "DECLARE" => Ok(Self::Declare),
            "DeployAccount" | "DEPLOY_ACCOUNT" => Ok(Self::DeployAccount),
            "InvokeFunction" | "INVOKE_FUNCTION" => Ok(Self::InvokeFunction),
            "L1Handler" | "L1_HANDLER" => Ok(Self::L1Handler),
            unknown_tx_type => Err(Self::Err::UnknownTransactionType(unknown_tx_type.to_string())),
        }
    }
}

impl std::fmt::Display for TransactionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let type_str = match self {
            Self::Declare => "DECLARE",
            Self::DeployAccount => "DEPLOY_ACCOUNT",
            Self::InvokeFunction => "INVOKE_FUNCTION",
            Self::L1Handler => "L1_HANDLER",
        };
        write!(f, "{type_str}")
    }
}

impl TransactionType {
    pub fn tx_type_as_felt(&self) -> Felt {
        let tx_type_name = self.to_string();
        Felt::from_bytes_be_slice(tx_type_name.as_bytes())
    }
}

/// Represents a paid Starknet transaction.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AccountTransaction {
    Declare(DeclareTransaction),
    DeployAccount(DeployAccountTransaction),
    Invoke(InvokeTransaction),
}

impl AccountTransaction {
    implement_account_tx_inner_getters!(
        (resource_bounds, ValidResourceBounds),
        (tip, Tip),
        (signature, TransactionSignature),
        (nonce, Nonce),
        (nonce_data_availability_mode, DataAvailabilityMode),
        (fee_data_availability_mode, DataAvailabilityMode),
        (paymaster_data, PaymasterData),
        (version, TransactionVersion)
    );

    pub fn contract_address(&self) -> ContractAddress {
        match self {
            AccountTransaction::Declare(tx_data) => tx_data.tx.sender_address(),
            AccountTransaction::DeployAccount(tx_data) => tx_data.contract_address,
            AccountTransaction::Invoke(tx_data) => tx_data.tx.sender_address(),
        }
    }

    pub fn sender_address(&self) -> ContractAddress {
        self.contract_address()
    }

    pub fn tx_hash(&self) -> TransactionHash {
        match self {
            AccountTransaction::Declare(tx_data) => tx_data.tx_hash,
            AccountTransaction::DeployAccount(tx_data) => tx_data.tx_hash,
            AccountTransaction::Invoke(tx_data) => tx_data.tx_hash,
        }
    }

    pub fn tx_type(&self) -> TransactionType {
        match self {
            Self::Declare(_) => TransactionType::Declare,
            Self::DeployAccount(_) => TransactionType::DeployAccount,
            Self::Invoke(_) => TransactionType::InvokeFunction,
        }
    }
}

// TODO(Mohammad): Add constructor for all the transaction's structs.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DeclareTransaction {
    pub tx: crate::transaction::DeclareTransaction,
    pub tx_hash: TransactionHash,
    pub class_info: ClassInfo,
}

impl DeclareTransaction {
    implement_inner_tx_getter_calls!(
        (class_hash, ClassHash),
        (nonce, Nonce),
        (sender_address, ContractAddress),
        (signature, TransactionSignature),
        (version, TransactionVersion),
        // compiled_class_hash is only supported in V2 and V3, otherwise the getter panics.
        (compiled_class_hash, CompiledClassHash),
        // The following fields are only supported in V3, otherwise the getter panics.
        (tip, Tip),
        (nonce_data_availability_mode, DataAvailabilityMode),
        (fee_data_availability_mode, DataAvailabilityMode),
        (paymaster_data, PaymasterData),
        (account_deployment_data, AccountDeploymentData),
        (resource_bounds, ValidResourceBounds)
    );

    pub fn create(
        declare_tx: crate::transaction::DeclareTransaction,
        class_info: ClassInfo,
        chain_id: &ChainId,
    ) -> Result<Self, StarknetApiError> {
        validate_class_version_matches_tx_version(
            declare_tx.version(),
            &class_info.contract_class,
        )?;
        let tx_hash = declare_tx.calculate_transaction_hash(chain_id, &declare_tx.version())?;
        Ok(Self { tx: declare_tx, tx_hash, class_info })
    }

    pub fn contract_class(&self) -> ContractClass {
        self.class_info.contract_class.clone()
    }

    /// Casm contract class exists only for contract class V1, for version V0 the getter panics.
    pub fn casm_contract_class(&self) -> &CasmContractClass {
        let ContractClass::V1(versioned_casm) = &self.class_info.contract_class else {
            panic!("Contract class version must be V1.")
        };
        &versioned_casm.0
    }

    // Returns whether the declare transaction is for bootstrapping.
    // In this case, no account-related actions should be made besides the declaration.
    pub fn is_bootstrap_declare(&self, charge_fee: bool) -> bool {
        if let crate::transaction::DeclareTransaction::V3(tx) = &self.tx {
            return tx.sender_address == Self::bootstrap_address()
                && tx.nonce == Nonce(Felt::ZERO)
                && !charge_fee;
        }
        false
    }

    /// Returns the address of the bootstrap contract.
    /// Declare transactions can be sent from this contract with no validation, fee or nonce
    /// change. This is used for starting a new Starknet system.
    pub fn bootstrap_address() -> ContractAddress {
        // A felt representation of the string 'BOOTSTRAP'.
        ContractAddress::from(0x424f4f545354524150_u128)
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ValidateCompiledClassHashError {
    #[error(
        "Computed compiled class hash: {:#x} does not match the given value: {:#x}.",
        computed_class_hash.0, supplied_class_hash.0
    )]
    CompiledClassHashMismatch {
        computed_class_hash: CompiledClassHash,
        supplied_class_hash: CompiledClassHash,
    },
}

/// Validates that the Declare transaction version is compatible with the Cairo contract version.
/// Versions 0 and 1 declare Cairo 0 contracts, while versions >=2 declare Cairo 1 contracts.
fn validate_class_version_matches_tx_version(
    declare_version: TransactionVersion,
    class: &ContractClass,
) -> Result<(), StarknetApiError> {
    let expected_cairo_version = if declare_version <= TransactionVersion::ONE { 0 } else { 1 };

    match class {
        ContractClass::V0(_) if expected_cairo_version == 0 => Ok(()),
        ContractClass::V1(_) if expected_cairo_version == 1 => Ok(()),
        _ => Err(StarknetApiError::ContractClassVersionMismatch {
            declare_version,
            cairo_version: expected_cairo_version,
        }),
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DeployAccountTransaction {
    pub tx: crate::transaction::DeployAccountTransaction,
    pub tx_hash: TransactionHash,
    pub contract_address: ContractAddress,
}

impl DeployAccountTransaction {
    implement_inner_tx_getter_calls!(
        (class_hash, ClassHash),
        (constructor_calldata, Calldata),
        (contract_address_salt, ContractAddressSalt),
        (nonce, Nonce),
        (signature, TransactionSignature),
        (version, TransactionVersion),
        (resource_bounds, ValidResourceBounds),
        (tip, Tip),
        (nonce_data_availability_mode, DataAvailabilityMode),
        (fee_data_availability_mode, DataAvailabilityMode),
        (paymaster_data, PaymasterData)
    );
    implement_getter_calls!((tx_hash, TransactionHash), (contract_address, ContractAddress));

    pub fn tx(&self) -> &crate::transaction::DeployAccountTransaction {
        &self.tx
    }

    pub fn create(
        deploy_account_tx: crate::transaction::DeployAccountTransaction,
        chain_id: &ChainId,
    ) -> Result<Self, StarknetApiError> {
        let contract_address = deploy_account_tx.calculate_contract_address()?;
        let tx_hash =
            deploy_account_tx.calculate_transaction_hash(chain_id, &deploy_account_tx.version())?;
        Ok(Self { tx: deploy_account_tx, tx_hash, contract_address })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InvokeTransaction {
    pub tx: crate::transaction::InvokeTransaction,
    pub tx_hash: TransactionHash,
}

impl InvokeTransaction {
    implement_inner_tx_getter_calls!(
        (calldata, Calldata),
        (nonce, Nonce),
        (signature, TransactionSignature),
        (sender_address, ContractAddress),
        (version, TransactionVersion),
        (resource_bounds, ValidResourceBounds),
        (tip, Tip),
        (nonce_data_availability_mode, DataAvailabilityMode),
        (fee_data_availability_mode, DataAvailabilityMode),
        (paymaster_data, PaymasterData),
        (account_deployment_data, AccountDeploymentData)
    );
    implement_getter_calls!((tx_hash, TransactionHash));

    pub fn tx(&self) -> &crate::transaction::InvokeTransaction {
        &self.tx
    }

    pub fn create(
        invoke_tx: crate::transaction::InvokeTransaction,
        chain_id: &ChainId,
    ) -> Result<Self, StarknetApiError> {
        let tx_hash = invoke_tx.calculate_transaction_hash(chain_id, &invoke_tx.version())?;
        Ok(Self { tx: invoke_tx, tx_hash })
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Hash)]
pub struct L1HandlerTransaction {
    pub tx: crate::transaction::L1HandlerTransaction,
    pub tx_hash: TransactionHash,
    pub paid_fee_on_l1: Fee,
}

impl L1HandlerTransaction {
    pub const L1_HANDLER_TYPE_NAME: &str = "L1_HANDLER";

    pub fn create(
        raw_tx: crate::transaction::L1HandlerTransaction,
        chain_id: &ChainId,
        paid_fee_on_l1: Fee,
    ) -> Result<L1HandlerTransaction, StarknetApiError> {
        let tx_hash = raw_tx.calculate_transaction_hash(chain_id, &raw_tx.version)?;
        Ok(Self { tx: raw_tx, tx_hash, paid_fee_on_l1 })
    }

    pub fn payload_size(&self) -> usize {
        // The calldata includes the "from" field, which is not a part of the payload.
        self.tx.calldata.0.len() - 1
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Transaction {
    Account(AccountTransaction),
    L1Handler(L1HandlerTransaction),
}

impl Transaction {
    pub fn tx_hash(&self) -> TransactionHash {
        match self {
            Self::Account(tx) => tx.tx_hash(),
            Self::L1Handler(tx) => tx.tx_hash,
        }
    }

    pub fn tx_type(&self) -> TransactionType {
        match self {
            Self::Account(account_tx) => account_tx.tx_type(),
            Self::L1Handler(_) => TransactionType::L1Handler,
        }
    }

    pub fn version(&self) -> TransactionVersion {
        match self {
            Self::Account(account_tx) => account_tx.version(),
            Self::L1Handler(l1_handler_tx) => l1_handler_tx.tx.version,
        }
    }

    pub fn nonce(&self) -> Nonce {
        match self {
            Self::Account(account_tx) => account_tx.nonce(),
            Self::L1Handler(l1_handler_tx) => l1_handler_tx.tx.nonce,
        }
    }
}
