//! Metrics for the Propeller protocol.
//!
//! This module provides comprehensive metrics for monitoring and profiling the Propeller
//! protocol's performance, particularly focused on shard throughput and message propagation.

#![allow(clippy::as_conversions)]

use std::time::Duration;

use apollo_metrics::generate_permutation_labels;
use apollo_metrics::metrics::{
    LabeledMetricCounter,
    LabeledMetricGauge,
    MetricCounter,
    MetricHistogram,
};
use strum::VariantNames;
use strum_macros::{EnumVariantNames, IntoStaticStr};

use crate::ShardValidationError;

// ================================================================================================
// Shard Validation Failure Reasons
// ================================================================================================

/// Label name for shard validation failure reasons
pub const LABEL_NAME_VALIDATION_FAILURE_REASON: &str = "failure_reason";

/// Reasons why shard validation can fail
#[derive(IntoStaticStr, EnumVariantNames)]
#[strum(serialize_all = "snake_case")]
pub enum ShardValidationFailureReason {
    /// Received our own published shard
    ReceivedPublishedShard,
    /// Duplicate shard already in cache
    DuplicateShard,
    /// Tree topology error
    TreeError,
    /// Parent verification failed (wrong sender)
    ParentVerificationFailed,
    /// Signature verification failed
    SignatureVerificationFailed,
    /// Proof verification failed
    ProofVerificationFailed,
}

generate_permutation_labels! {
    SHARD_VALIDATION_FAILURE_LABELS,
    (LABEL_NAME_VALIDATION_FAILURE_REASON, ShardValidationFailureReason),
}

impl From<ShardValidationError> for ShardValidationFailureReason {
    fn from(error: ShardValidationError) -> Self {
        match error {
            ShardValidationError::ReceivedPublishedShard => Self::ReceivedPublishedShard,
            ShardValidationError::DuplicateShard => Self::DuplicateShard,
            ShardValidationError::TreeError(_) => Self::TreeError,
            ShardValidationError::UnexpectedSender { .. } => Self::ParentVerificationFailed,
            ShardValidationError::SignatureVerificationFailed(_) => {
                Self::SignatureVerificationFailed
            }
            ShardValidationError::ProofVerificationFailed => Self::ProofVerificationFailed,
        }
    }
}

// ================================================================================================
// Shard Send Failure Reasons
// ================================================================================================

/// Label name for shard send failure reasons
pub const LABEL_NAME_SEND_FAILURE_REASON: &str = "failure_reason";

/// Reasons why shard sending can fail
#[derive(IntoStaticStr, EnumVariantNames)]
#[strum(serialize_all = "snake_case")]
pub enum ShardSendFailureReason {
    /// Local peer not in peer weights
    LocalPeerNotInPeerWeights,
    /// Invalid data size for encoding
    InvalidDataSize,
    /// Signing operation failed
    SigningFailed,
    /// Erasure encoding failed
    ErasureEncodingFailed,
    /// Not connected to target peer
    NotConnectedToPeer,
    /// Handler error
    HandlerError,
    /// Tree generation error
    TreeGenerationError,
}

generate_permutation_labels! {
    SHARD_SEND_FAILURE_LABELS,
    (LABEL_NAME_SEND_FAILURE_REASON, ShardSendFailureReason),
}

// ================================================================================================
// Collection Length Labels
// ================================================================================================

/// Label name for collection length tracking
pub const LABEL_NAME_COLLECTION: &str = "collection";

/// Collections that are tracked for size/length
#[derive(IntoStaticStr, EnumVariantNames)]
#[strum(serialize_all = "snake_case")]
pub enum CollectionLabel {
    /// Pending events queue
    EventsQueue,
    /// Connected peers set
    ConnectedPeers,
    /// Active message processors
    ActiveProcessors,
    /// Finalized messages cache across all channels
    FinalizedMessages,
    /// Registered channels
    RegisteredChannels,
}

generate_permutation_labels! {
    COLLECTION_LENGTH_LABELS,
    (LABEL_NAME_COLLECTION, CollectionLabel),
}

// ================================================================================================
// Propeller Metrics
// ================================================================================================

/// Comprehensive metrics for the Propeller protocol
///
/// These metrics are designed to enable performance profiling and monitoring of:
/// - Shard throughput (sent/received per second)
/// - Validation success rates
/// - Message reconstruction rates
/// - Network health (connected peers, cache sizes)
pub struct PropellerMetrics {
    // Shard Publishing Metrics
    /// Total number of shards published (created) by this node
    pub shards_published: MetricCounter,

    /// Total number of shards sent to peers (includes forwarding)
    pub shards_sent: MetricCounter,

    /// Total number of shard send failures, labeled by reason
    pub shards_send_failed: LabeledMetricCounter,

    /// Total bytes sent in shard data (payload only, excluding protocol overhead)
    pub shard_bytes_sent: MetricCounter,

    // Shard Receiving Metrics
    /// Total number of shards received from peers
    pub shards_received: MetricCounter,

    /// Total number of shards successfully validated
    pub shards_validated: MetricCounter,

    /// Total number of shards that failed validation, labeled by reason
    pub shards_validation_failed: LabeledMetricCounter,

    /// Total number of shards forwarded to children in tree
    pub shards_forwarded: MetricCounter,

    /// Total bytes received in shard data (payload only, excluding protocol overhead)
    pub shard_bytes_received: MetricCounter,

    // Message Reconstruction Metrics
    /// Total number of messages successfully reconstructed from shards
    pub messages_reconstructed: MetricCounter,

    /// Total number of message reconstruction failures
    pub messages_reconstruction_failed: MetricCounter,

    // Tree Topology Metrics
    /// Total number of tree generation operations
    pub trees_generated: MetricCounter,

    // Collection Size Metrics
    /// Length of various collections (queues, sets, caches) tracked by label
    pub collection_lengths: LabeledMetricGauge,

    // Timing Metrics
    /// Time to validate a single shard (seconds)
    pub shard_validation_duration: MetricHistogram,

    /// Time to reconstruct a message from shards (seconds)
    pub message_reconstruction_duration: MetricHistogram,

    /// End-to-end latency from first shard received to message reconstructed (seconds)
    pub message_end_to_end_latency: MetricHistogram,
}

impl PropellerMetrics {
    /// Register all metrics with the metrics system
    pub fn register(&self) {
        // Shard publishing metrics
        self.shards_published.register();
        self.shards_sent.register();
        self.shards_send_failed.register();
        self.shard_bytes_sent.register();

        // Shard receiving metrics
        self.shards_received.register();
        self.shards_validated.register();
        self.shards_validation_failed.register();
        self.shards_forwarded.register();
        self.shard_bytes_received.register();

        // Message reconstruction metrics
        self.messages_reconstructed.register();
        self.messages_reconstruction_failed.register();

        // Tree topology metrics
        self.trees_generated.register();

        // Collection size metrics
        self.collection_lengths.register();
        // Initialize all collection length gauges to 0
        for variant in CollectionLabel::VARIANTS {
            self.collection_lengths.set(0f64, &[(LABEL_NAME_COLLECTION, *variant)]);
        }

        // Timing metrics
        self.shard_validation_duration.register();
        self.message_reconstruction_duration.register();
        self.message_end_to_end_latency.register();
    }

    /// Increment the validation failure counter for a specific reason
    pub fn increment_validation_failure(&self, reason: ShardValidationFailureReason) {
        self.shards_validation_failed
            .increment(1, &[(LABEL_NAME_VALIDATION_FAILURE_REASON, reason.into())]);
    }

    /// Increment the send failure counter for a specific reason
    pub fn increment_send_failure(&self, reason: ShardSendFailureReason) {
        self.shards_send_failed.increment(1, &[(LABEL_NAME_SEND_FAILURE_REASON, reason.into())]);
    }

    /// Record the duration of a shard validation operation
    pub fn record_validation_duration(&self, duration: Duration) {
        self.shard_validation_duration.record(duration.as_secs_f64());
    }

    /// Record the duration of a message reconstruction operation
    pub fn record_reconstruction_duration(&self, duration: Duration) {
        self.message_reconstruction_duration.record(duration.as_secs_f64());
    }

    /// Record the end-to-end latency from first shard received to message reconstructed
    pub fn record_end_to_end_latency(&self, duration: Duration) {
        self.message_end_to_end_latency.record(duration.as_secs_f64());
    }

    /// Update the length/size of a specific collection
    pub fn update_collection_length(&self, label: CollectionLabel, size: usize) {
        self.collection_lengths.set(size as f64, &[(LABEL_NAME_COLLECTION, label.into())]);
    }
}
