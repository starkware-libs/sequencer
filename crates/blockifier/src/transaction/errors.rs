use cairo_vm::types::errors::program_errors::ProgramError;
use num_bigint::BigUint;
use starknet_api::block::GasPrice;
use starknet_api::core::{ClassHash, ContractAddress, EntryPointSelector, Nonce};
use starknet_api::execution_resources::GasAmount;
use starknet_api::transaction::fields::{Fee, Resource, TransactionVersion};
use starknet_api::StarknetApiError;
use starknet_types_core::felt::FromStrError;
use thiserror::Error;

use crate::bouncer::BouncerWeights;
use crate::execution::call_info::Retdata;
use crate::execution::errors::{ConstructorEntryPointExecutionError, EntryPointExecutionError};
use crate::execution::execution_utils::format_panic_data;
use crate::execution::stack_trace::gen_tx_execution_error_trace;
use crate::fee::fee_checks::FeeCheckError;
use crate::state::errors::StateError;

// TODO(Yoni, 1/9/2024): implement Display for Fee.
#[derive(Debug, Error)]
pub enum TransactionFeeError {
    #[error("Cairo resource names must be contained in fee cost dict.")]
    CairoResourcesNotContainedInFeeCosts,
    #[error(transparent)]
    ExecuteFeeTransferError(#[from] EntryPointExecutionError),
    #[error("Actual fee ({}) exceeded max fee ({}).", actual_fee.0, max_fee.0)]
    FeeTransferError { max_fee: Fee, actual_fee: Fee },
    #[error("Actual fee ({}) exceeded paid fee on L1 ({}).", actual_fee.0, paid_fee.0)]
    InsufficientFee { paid_fee: Fee, actual_fee: Fee },
    #[error(
        "Resources bounds (l1 gas max amount: {l1_max_amount}, l1 gas max price: {l1_max_price}, \
         l1 data max amount: {l1_data_max_amount}, l1 data max price: {l1_data_max_price}, l2 gas \
         max amount: {l2_max_amount}, l2 gas max price: {l2_max_price}) exceed balance \
         ({balance})."
    )]
    ResourcesBoundsExceedBalance {
        l1_max_amount: GasAmount,
        l1_max_price: GasPrice,
        l1_data_max_amount: GasAmount,
        l1_data_max_price: GasPrice,
        l2_max_amount: GasAmount,
        l2_max_price: GasPrice,
        balance: BigUint,
    },
    #[error(
        "Resource {resource} bounds (max amount: {max_amount}, max price): {max_price}) exceed \
         balance ({balance})."
    )]
    GasBoundsExceedBalance {
        resource: Resource,
        max_amount: GasAmount,
        max_price: GasPrice,
        balance: BigUint,
    },
    #[error("Max fee ({}) exceeds balance ({balance}).", max_fee.0, )]
    MaxFeeExceedsBalance { max_fee: Fee, balance: BigUint },
    #[error("Max fee ({}) is too low. Minimum fee: {}.", max_fee.0, min_fee.0)]
    MaxFeeTooLow { min_fee: Fee, max_fee: Fee },
    #[error(
        "Max {resource} price ({max_gas_price}) is lower than the actual gas price: \
         {actual_gas_price}."
    )]
    MaxGasPriceTooLow { resource: Resource, max_gas_price: GasPrice, actual_gas_price: GasPrice },
    #[error(
        "Max {resource} amount ({max_gas_amount}) is lower than the minimal gas amount: \
         {minimal_gas_amount}."
    )]
    MaxGasAmountTooLow {
        resource: Resource,
        max_gas_amount: GasAmount,
        minimal_gas_amount: GasAmount,
    },
    #[error("Missing L1 gas bounds in resource bounds.")]
    MissingL1GasBounds,
    #[error(transparent)]
    StateError(#[from] StateError),
}

#[derive(Debug, Error)]
pub enum TransactionExecutionError {
    #[error(
        "Declare transaction version {} must have a contract class of Cairo \
         version {cairo_version:?}.", **declare_version
    )]
    ContractClassVersionMismatch { declare_version: TransactionVersion, cairo_version: u64 },
    #[error(
        "Contract constructor execution has failed:\n{}",
        String::from(gen_tx_execution_error_trace(self))
    )]
    ContractConstructorExecutionFailed(#[from] ConstructorEntryPointExecutionError),
    #[error("Class with hash {:#064x} is already declared.", **class_hash)]
    DeclareTransactionError { class_hash: ClassHash },
    #[error(
        "Transaction execution has failed:\n{}",
        String::from(gen_tx_execution_error_trace(self))
    )]
    ExecutionError {
        error: EntryPointExecutionError,
        class_hash: ClassHash,
        storage_address: ContractAddress,
        selector: EntryPointSelector,
    },
    #[error(transparent)]
    FeeCheckError(#[from] FeeCheckError),
    #[error(transparent)]
    FromStr(#[from] FromStrError),
    #[error("The `validate` entry point panicked with {}.", format_panic_data(&panic_reason.0))]
    PanicInValidate { panic_reason: Retdata },
    #[error("The `validate` entry point should return `VALID`. Got {actual:?}.")]
    InvalidValidateReturnData { actual: Retdata },
    #[error(
        "Transaction version {:?} is not supported. Supported versions: \
         {:?}.", **version, allowed_versions.iter().map(|v| **v).collect::<Vec<_>>()
    )]
    InvalidVersion { version: TransactionVersion, allowed_versions: Vec<TransactionVersion> },
    #[error(transparent)]
    StarknetApiError(#[from] StarknetApiError),
    #[error(transparent)]
    StateError(#[from] StateError),
    #[error(transparent)]
    TransactionFeeError(#[from] TransactionFeeError),
    #[error(transparent)]
    TransactionPreValidationError(#[from] TransactionPreValidationError),
    #[error(transparent)]
    TryFromIntError(#[from] std::num::TryFromIntError),
    #[error(
        "Transaction size exceeds the maximum block capacity. Max block capacity: {}, \
         transaction size: {}.", *max_capacity, *tx_size
    )]
    TransactionTooLarge { max_capacity: Box<BouncerWeights>, tx_size: Box<BouncerWeights> },
    #[error(
        "Transaction validation has failed:\n{}",
        String::from(gen_tx_execution_error_trace(self))
    )]
    ValidateTransactionError {
        error: EntryPointExecutionError,
        class_hash: ClassHash,
        storage_address: ContractAddress,
        selector: EntryPointSelector,
    },
    #[error(
        "Invalid segment structure: PC {0} was visited, but the beginning of the segment {1} was \
         not."
    )]
    InvalidSegmentStructure(usize, usize),
    #[error(transparent)]
    ProgramError(#[from] ProgramError),
}

#[derive(Debug, Error)]
pub enum TransactionPreValidationError {
    #[error(
        "Invalid transaction nonce of contract at address {:#064x}. Account nonce: \
         {:#064x}; got: {:#064x}.", ***address, **account_nonce, **incoming_tx_nonce
    )]
    InvalidNonce { address: ContractAddress, account_nonce: Nonce, incoming_tx_nonce: Nonce },
    #[error(transparent)]
    StateError(#[from] StateError),
    #[error(transparent)]
    TransactionFeeError(#[from] TransactionFeeError),
}

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("Unsupported transaction type: {0}")]
    UnknownTransactionType(String),
}

#[derive(Debug, Error)]
pub enum NumericConversionError {
    #[error("Conversion of {0} to u128 unsuccessful.")]
    U128ToUsizeError(u128),
    #[error("Conversion of {0} to u64 unsuccessful.")]
    U64ToUsizeError(u64),
}
