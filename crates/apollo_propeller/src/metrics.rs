//! Metrics for the Propeller protocol.
//!
//! This module provides comprehensive metrics for monitoring and profiling the Propeller
//! protocol's performance, particularly focused on shard throughput and message propagation.

#![allow(clippy::as_conversions)]

use apollo_metrics::generate_permutation_labels;
use apollo_metrics::metrics::{LabeledMetricCounter, MetricCounter, MetricGauge};
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

    // Connection Metrics
    /// Current number of connected peers
    pub num_connected_peers: MetricGauge,

    // Cache Metrics
    /// Current number of shards in the verified shards cache
    pub cached_verified_shards: MetricGauge,

    /// Current number of message IDs in the reconstructed messages cache
    pub cached_reconstructed_messages: MetricGauge,
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

        // Connection metrics
        self.num_connected_peers.register();
        self.num_connected_peers.set(0f64);

        // Cache metrics
        self.cached_verified_shards.register();
        self.cached_verified_shards.set(0f64);
        self.cached_reconstructed_messages.register();
        self.cached_reconstructed_messages.set(0f64);
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

    /// Update cache size gauges based on current cache state
    pub fn update_cache_sizes(
        &self,
        verified_shards_count: usize,
        reconstructed_messages_count: usize,
    ) {
        self.cached_verified_shards.set(verified_shards_count as f64);
        self.cached_reconstructed_messages.set(reconstructed_messages_count as f64);
    }
}
