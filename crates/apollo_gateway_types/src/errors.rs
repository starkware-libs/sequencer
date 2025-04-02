use apollo_network_types::network_types::BroadcastedMessageMetadata;
use apollo_rpc::error::{
    unexpected_error,
    validation_failure,
    JsonRpcError,
    CLASS_ALREADY_DECLARED,
    CLASS_HASH_NOT_FOUND,
    COMPILATION_FAILED,
    COMPILED_CLASS_HASH_MISMATCH,
    CONTRACT_CLASS_SIZE_IS_TOO_LARGE,
    DUPLICATE_TX,
    INSUFFICIENT_ACCOUNT_BALANCE,
    INSUFFICIENT_MAX_FEE,
    INVALID_TRANSACTION_NONCE,
    NON_ACCOUNT,
    UNSUPPORTED_CONTRACT_CLASS_VERSION,
    UNSUPPORTED_TX_VERSION,
};
use enum_assoc::Assoc;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Error returned by the gateway, adhering to the Starknet RPC error format.
// To get JsonRpcError from GatewaySpecError, use `into_rpc` method.
// TODO(yair): apollo_rpc has a test that the add_tx functions return the correct error. Make sure
// it is tested when we have a single gateway.
#[derive(Debug, Clone, Eq, PartialEq, Assoc, Error, Serialize, Deserialize)]
#[func(pub fn into_rpc(self) -> JsonRpcError<String>)]
pub enum GatewaySpecError {
    #[assoc(into_rpc = CLASS_ALREADY_DECLARED)]
    ClassAlreadyDeclared,
    #[assoc(into_rpc = CLASS_HASH_NOT_FOUND)]
    ClassHashNotFound,
    #[assoc(into_rpc = COMPILED_CLASS_HASH_MISMATCH)]
    CompiledClassHashMismatch,
    #[assoc(into_rpc = COMPILATION_FAILED)]
    CompilationFailed,
    #[assoc(into_rpc = CONTRACT_CLASS_SIZE_IS_TOO_LARGE)]
    ContractClassSizeIsTooLarge,
    #[assoc(into_rpc = DUPLICATE_TX)]
    DuplicateTx,
    #[assoc(into_rpc = INSUFFICIENT_ACCOUNT_BALANCE)]
    InsufficientAccountBalance,
    #[assoc(into_rpc = INSUFFICIENT_MAX_FEE)]
    InsufficientMaxFee,
    #[assoc(into_rpc = INVALID_TRANSACTION_NONCE)]
    InvalidTransactionNonce,
    #[assoc(into_rpc = NON_ACCOUNT)]
    NonAccount,
    #[assoc(into_rpc = unexpected_error(_data))]
    UnexpectedError { data: String },
    #[assoc(into_rpc = UNSUPPORTED_CONTRACT_CLASS_VERSION)]
    UnsupportedContractClassVersion,
    #[assoc(into_rpc = UNSUPPORTED_TX_VERSION)]
    UnsupportedTxVersion,
    #[assoc(into_rpc = validation_failure(_data))]
    ValidationFailure { data: String },
}

impl std::fmt::Display for GatewaySpecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let as_rpc = self.clone().into_rpc();
        write!(
            f,
            "{}: {}. data: {}",
            as_rpc.code,
            as_rpc.message,
            serde_json::to_string(&as_rpc.data).unwrap()
        )
    }
}

pub struct DeprecatedGatewayError {
    pub code: String,
    pub message: String,
}

impl GatewaySpecError {
    pub fn to_deprecated_gw_error(&self) -> DeprecatedGatewayError {
        const BLOCKIFIER_ERROR: &str = "VALIDATE_FAILURE"; // All errors from the blockifier are validation failures.
        let code;
        let message;
        match self {
            GatewaySpecError::ClassAlreadyDeclared => {
                code = BLOCKIFIER_ERROR;
                // Note: In py GW the format is "Class with hash {:#064x} is already declared."
                message = "Class is already declared.";
            }
            GatewaySpecError::ClassHashNotFound => {
                code = BLOCKIFIER_ERROR;
                // Note: In py GW the format is "Class with hash {:#064x} is not declared."
                message = "Class is not declared.";
            }
            GatewaySpecError::CompiledClassHashMismatch => {
                code = BLOCKIFIER_ERROR;
                // Note: In py GW the format is "CASM and Sierra mismatch for class hash
                // {:#064x}: {message}."
                message = "CASM and Sierra mismatch.";
            }
            GatewaySpecError::CompilationFailed => {
                code = "COMPILATION_FAILED";
                // Note: In py GW it follows by the reason.
                message = "Compilation failed.";
            }
            GatewaySpecError::ContractClassSizeIsTooLarge => {
                code = "CONTRACT_CLASS_OBJECT_SIZE_TOO_LARGE";
                // Note: In py GW the format is "Cannot declare contract class with size of {}; max
                // allowed size: {}".
                message = "Cannot declare contract class, size is too large.";
            }
            GatewaySpecError::DuplicateTx => {
                code = "DUPLICATED_TRANSACTION";
                // Note: In py GW the format is "Transaction with hash {} already exists."
                message = "Transaction already exists.";
            }
            GatewaySpecError::InsufficientAccountBalance => {
                code = BLOCKIFIER_ERROR;
                // Note: In py GW the format is "Resources bounds ({bounds}) exceed balance
                // ({balance})."
                message = "Resources bounds exceed balance.";
            }
            GatewaySpecError::InsufficientMaxFee => {
                code = BLOCKIFIER_ERROR;
                // Note: In py GW the format is "Max fee ({}) is too low. Minimum fee: {}."
                message = "Max fee is too low.";
            }
            GatewaySpecError::InvalidTransactionNonce => {
                code = BLOCKIFIER_ERROR;
                // Note: In py GW the format is "Invalid transaction nonce of contract at address
                // {:#064x}. Account nonce: {:#064x}; got: {:#064x}.".
                message = "Invalid transaction nonce.";
            }
            GatewaySpecError::NonAccount => {
                unreachable!("NonAccount error is not expected to be returned from the gateway.");
            }
            GatewaySpecError::UnexpectedError { data } => {
                code = "UNEXPECTED_FAILURE";
                message = data;
            }
            GatewaySpecError::UnsupportedContractClassVersion => {
                code = "INVALID_CONTRACT_CLASS_VERSION";
                // Note: In py GW followed by "Expected {contract_class_version_ident_str}; got
                // CONTRACT_CLASS_V{contract_class_version}."
                message = "Unexpected contract class version.";
            }
            GatewaySpecError::UnsupportedTxVersion => {
                code = "INVALID_TRANSACTION_VERSION";
                // Note: In py GW the format is "Transaction version {version} is not supported...
                message = "Transaction version is not supported. Supported versions: [3].";
            }
            GatewaySpecError::ValidationFailure { data } => {
                code = BLOCKIFIER_ERROR;
                message = data;
            }
        }
        DeprecatedGatewayError {
            code: format!("StarknetErrorCode.{code}"),
            message: message.to_string(),
        }
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatewayError {
    #[error("{source:?}")]
    GatewaySpecError {
        source: GatewaySpecError,
        p2p_message_metadata: Option<BroadcastedMessageMetadata>,
    },
}
