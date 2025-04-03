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

use crate::deprecated_gw_error::StarknetError;

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

#[derive(Clone, Debug, Error, PartialEq, Eq, Serialize, Deserialize)]
pub enum GatewayError {
    #[error("{source:?}")]
    DeprecatedGWError {
        source: StarknetError,
        p2p_message_metadata: Option<BroadcastedMessageMetadata>,
    },
}
