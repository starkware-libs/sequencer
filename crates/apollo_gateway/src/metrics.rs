use apollo_gateway_types::deprecated_gateway_error::{
    KnownStarknetErrorCode,
    StarknetError,
    StarknetErrorCode,
};
use apollo_infra::metrics::{
    InfraMetrics,
    LocalClientMetrics,
    LocalServerMetrics,
    RemoteClientMetrics,
    RemoteServerMetrics,
};
#[cfg(test)]
use apollo_metrics::metrics::LabeledMetricCounter;
use apollo_metrics::{define_infra_metrics, define_metrics, generate_permutation_labels};
use apollo_network_types::network_types::BroadcastedMessageMetadata;
use starknet_api::rpc_transaction::{RpcTransaction, RpcTransactionLabelValue};
use strum::{EnumVariantNames, VariantNames};
use strum_macros::IntoStaticStr;

use crate::communication::GATEWAY_REQUEST_LABELS;

pub const LABEL_NAME_TX_TYPE: &str = "tx_type";
pub const LABEL_NAME_SOURCE: &str = "source";
pub const LABEL_NAME_ADD_TX_FAILURE_REASON: &str = "add_tx_failure_reason";

generate_permutation_labels! {
    TRANSACTION_TYPE_AND_SOURCE_LABELS,
    (LABEL_NAME_TX_TYPE, RpcTransactionLabelValue),
    (LABEL_NAME_SOURCE, SourceLabelValue),
}

generate_permutation_labels! {
    ADD_TX_FAILURE_LABELS,
    (LABEL_NAME_ADD_TX_FAILURE_REASON, GatewayAddTxFailureReason),
}

define_infra_metrics!(gateway);

define_metrics!(
    Gateway => {
        LabeledMetricCounter { GATEWAY_TRANSACTIONS_RECEIVED, "gateway_transactions_received", "Counter of transactions received", init = 0 , labels = TRANSACTION_TYPE_AND_SOURCE_LABELS},
        LabeledMetricCounter { GATEWAY_TRANSACTIONS_FAILED, "gateway_transactions_failed", "Counter of failed transactions", init = 0 , labels = TRANSACTION_TYPE_AND_SOURCE_LABELS},
        LabeledMetricCounter { GATEWAY_TRANSACTIONS_SENT_TO_MEMPOOL, "gateway_transactions_sent_to_mempool", "Counter of transactions sent to the mempool", init = 0 , labels = TRANSACTION_TYPE_AND_SOURCE_LABELS},
        LabeledMetricCounter { GATEWAY_ADD_TX_FAILURE, "gateway_add_tx_failure", "Counter of add_tx failures by reason", init = 0 , labels = ADD_TX_FAILURE_LABELS},
        MetricHistogram { GATEWAY_ADD_TX_LATENCY, "gateway_add_tx_latency", "Latency of gateway add_tx function in secs" },
        MetricHistogram { GATEWAY_VALIDATE_TX_LATENCY, "gateway_validate_tx_latency", "Latency of gateway validate function in secs" },
        MetricHistogram { GATEWAY_VALIDATE_STATEFUL_TX_STORAGE_TIME, "gateway_validate_stateful_tx_storage_time", "Total time spent in storage operations in secs during stateful tx validation" },
        MetricHistogram { GATEWAY_VALIDATE_STATEFUL_TX_STORAGE_OPERATIONS, "gateway_validate_stateful_tx_storage_operations", "Total number of storage operations during stateful tx validation"},
    },
);

#[derive(Clone, Copy, Debug, IntoStaticStr, EnumVariantNames)]
#[strum(serialize_all = "snake_case")]
pub enum SourceLabelValue {
    Http,
    P2p,
}

#[derive(Clone, Copy, Debug, IntoStaticStr, EnumVariantNames)]
#[strum(serialize_all = "snake_case")]
pub enum GatewayAddTxFailureReason {
    // Starknet errors (1:1 with KnownStarknetErrorCode)
    UndeclaredClass,
    BlockNotFound,
    MalformedRequest,
    OutOfRangeClassHash,
    ClassAlreadyDeclared,
    CompilationFailed,
    ContractBytecodeSizeTooLarge,
    ContractClassObjectSizeTooLarge,
    DuplicatedTransaction,
    EntryPointNotFoundInContract,
    InsufficientAccountBalance,
    InsufficientMaxFee,
    InvalidCompiledClassHash,
    InvalidContractClassVersion,
    InvalidTransactionNonce,
    InvalidTransactionVersion,
    ValidateFailure,
    TransactionLimitExceeded,
    UnauthorizedDeclare,

    // Additional explicit UnknownErrorCode strings
    InvalidContractClass,
    CalldataTooLong,
    EntryPointsNotUniquelySorted,
    InvalidDataAvailabilityMode,
    InvalidSierraVersion,
    NonEmptyField,
    SignatureTooLong,
    StarknetApiError,
    MaxGasAmountTooHigh,
    NonceTooLarge,
    BlockedTransactionType,
    InternalError,
    UnknownStarknetError,
}

enum TransactionStatus {
    SentToMempool,
    Failed,
}

pub(crate) struct GatewayMetricHandle {
    tx_type: RpcTransactionLabelValue,
    source: SourceLabelValue,
    tx_status: TransactionStatus,
}

impl GatewayMetricHandle {
    pub fn new(
        tx: &RpcTransaction,
        p2p_message_metadata: &Option<BroadcastedMessageMetadata>,
    ) -> Self {
        let tx_type = RpcTransactionLabelValue::from(tx);
        let source = match p2p_message_metadata {
            Some(_) => SourceLabelValue::P2p,
            None => SourceLabelValue::Http,
        };
        Self { tx_type, source, tx_status: TransactionStatus::Failed }
    }

    fn label(&self) -> Vec<(&'static str, &'static str)> {
        vec![(LABEL_NAME_TX_TYPE, self.tx_type.into()), (LABEL_NAME_SOURCE, self.source.into())]
    }

    pub fn count_transaction_received(&self) {
        GATEWAY_TRANSACTIONS_RECEIVED.increment(1, &self.label());
    }

    pub fn transaction_sent_to_mempool(&mut self) {
        self.tx_status = TransactionStatus::SentToMempool;
    }

    pub fn record_add_tx_failure(&self, e: &StarknetError) {
        let reason = map_starknet_error_to_gateway_add_tx_failure_reason(e);
        GATEWAY_ADD_TX_FAILURE.increment(1, &[(LABEL_NAME_ADD_TX_FAILURE_REASON, reason.into())]);
    }

    #[cfg(test)]
    pub fn get_metric_value(&self, metric_counter: LabeledMetricCounter, metrics: &str) -> u64 {
        metric_counter.parse_numeric_metric::<u64>(metrics, &self.label()).unwrap()
    }
}

fn map_starknet_error_to_gateway_add_tx_failure_reason(
    e: &StarknetError,
) -> GatewayAddTxFailureReason {
    match &e.code {
        StarknetErrorCode::KnownErrorCode(code) => match code {
            KnownStarknetErrorCode::UndeclaredClass => GatewayAddTxFailureReason::UndeclaredClass,
            KnownStarknetErrorCode::BlockNotFound => GatewayAddTxFailureReason::BlockNotFound,
            KnownStarknetErrorCode::MalformedRequest => GatewayAddTxFailureReason::MalformedRequest,
            KnownStarknetErrorCode::OutOfRangeClassHash => {
                GatewayAddTxFailureReason::OutOfRangeClassHash
            }
            KnownStarknetErrorCode::ClassAlreadyDeclared => {
                GatewayAddTxFailureReason::ClassAlreadyDeclared
            }
            KnownStarknetErrorCode::CompilationFailed => {
                GatewayAddTxFailureReason::CompilationFailed
            }
            KnownStarknetErrorCode::ContractBytecodeSizeTooLarge => {
                GatewayAddTxFailureReason::ContractBytecodeSizeTooLarge
            }
            KnownStarknetErrorCode::ContractClassObjectSizeTooLarge => {
                GatewayAddTxFailureReason::ContractClassObjectSizeTooLarge
            }
            KnownStarknetErrorCode::DuplicatedTransaction => {
                GatewayAddTxFailureReason::DuplicatedTransaction
            }
            KnownStarknetErrorCode::EntryPointNotFoundInContract => {
                GatewayAddTxFailureReason::EntryPointNotFoundInContract
            }
            KnownStarknetErrorCode::InsufficientAccountBalance => {
                GatewayAddTxFailureReason::InsufficientAccountBalance
            }
            KnownStarknetErrorCode::InsufficientMaxFee => {
                GatewayAddTxFailureReason::InsufficientMaxFee
            }
            KnownStarknetErrorCode::InvalidCompiledClassHash => {
                GatewayAddTxFailureReason::InvalidCompiledClassHash
            }
            KnownStarknetErrorCode::InvalidContractClassVersion => {
                GatewayAddTxFailureReason::InvalidContractClassVersion
            }
            KnownStarknetErrorCode::InvalidTransactionNonce => {
                GatewayAddTxFailureReason::InvalidTransactionNonce
            }
            KnownStarknetErrorCode::InvalidTransactionVersion => {
                GatewayAddTxFailureReason::InvalidTransactionVersion
            }
            KnownStarknetErrorCode::ValidateFailure => GatewayAddTxFailureReason::ValidateFailure,
            KnownStarknetErrorCode::TransactionLimitExceeded => {
                GatewayAddTxFailureReason::TransactionLimitExceeded
            }
            KnownStarknetErrorCode::UnauthorizedDeclare => {
                GatewayAddTxFailureReason::UnauthorizedDeclare
            }
        },
        // TODO(Asmaa): Find better way to map unknown error codes to failure reasons
        StarknetErrorCode::UnknownErrorCode(s) => {
            if s.contains("BLOCKED_TRANSACTION_TYPE") {
                GatewayAddTxFailureReason::BlockedTransactionType
            } else if s.contains("INVALID_CONTRACT_CLASS") {
                GatewayAddTxFailureReason::InvalidContractClass
            } else if s.contains("CALLDATA_TOO_LONG") {
                GatewayAddTxFailureReason::CalldataTooLong
            } else if s.contains("ENTRY_POINTS_NOT_UNIQUELY_SORTED") {
                GatewayAddTxFailureReason::EntryPointsNotUniquelySorted
            } else if s.contains("INVALID_DATA_AVAILABILITY_MODE") {
                GatewayAddTxFailureReason::InvalidDataAvailabilityMode
            } else if s.contains("INVALID_SIERRA_VERSION") {
                GatewayAddTxFailureReason::InvalidSierraVersion
            } else if s.contains("NON_EMPTY_FIELD") {
                GatewayAddTxFailureReason::NonEmptyField
            } else if s.contains("SIGNATURE_TOO_LONG") {
                GatewayAddTxFailureReason::SignatureTooLong
            } else if s.contains("STARKNET_API_ERROR") {
                GatewayAddTxFailureReason::StarknetApiError
            } else if s.contains("MAX_GAS_AMOUNT_TOO_HIGH") {
                GatewayAddTxFailureReason::MaxGasAmountTooHigh
            } else if s.contains("NONCE_TOO_LARGE") {
                GatewayAddTxFailureReason::NonceTooLarge
            } else if s.contains("InternalError") {
                GatewayAddTxFailureReason::InternalError
            } else {
                GatewayAddTxFailureReason::UnknownStarknetError
            }
        }
    }
}

impl Drop for GatewayMetricHandle {
    fn drop(&mut self) {
        match self.tx_status {
            TransactionStatus::SentToMempool => {
                GATEWAY_TRANSACTIONS_SENT_TO_MEMPOOL.increment(1, &self.label())
            }
            TransactionStatus::Failed => GATEWAY_TRANSACTIONS_FAILED.increment(1, &self.label()),
        }
    }
}

pub(crate) fn register_metrics() {
    GATEWAY_TRANSACTIONS_RECEIVED.register();
    GATEWAY_TRANSACTIONS_FAILED.register();
    GATEWAY_TRANSACTIONS_SENT_TO_MEMPOOL.register();
    GATEWAY_ADD_TX_FAILURE.register();
    GATEWAY_ADD_TX_LATENCY.register();
    GATEWAY_VALIDATE_TX_LATENCY.register();
    GATEWAY_VALIDATE_STATEFUL_TX_STORAGE_TIME.register();
    GATEWAY_VALIDATE_STATEFUL_TX_STORAGE_OPERATIONS.register();
}
