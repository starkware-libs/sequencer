//! Metrics for the Propeller protocol.
//!
//! This module provides metrics for monitoring the Propeller protocol's performance,
//! particularly focused on shard throughput and message propagation.

use apollo_metrics::metrics::{
    LabeledMetricCounter,
    LabeledMetricGauge,
    LossyIntoF64,
    MetricCounter,
};
use strum::{IntoStaticStr, VariantNames};

use crate::types::{Event, ShardPublishError};
use crate::ShardValidationError;

/// Label name for shard validation failure reasons
pub const LABEL_NAME_VALIDATION_FAILURE_REASON: &str = "failure_reason";

/// Reasons why shard validation can fail
#[derive(IntoStaticStr, VariantNames)]
#[strum(serialize_all = "snake_case")]
pub enum ShardValidationFailureReason {
    /// Received a shard from ourselves via libp2p loopback
    SelfSending,
    /// Publisher received their own published shard back
    ReceivedSelfPublishedShard,
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
    /// Shards have inconsistent lengths
    UnequalShardLengths,
    /// Unexpected shard count per unit
    UnexpectedShardCount,
}

impl From<&ShardValidationError> for ShardValidationFailureReason {
    fn from(error: &ShardValidationError) -> Self {
        match error {
            ShardValidationError::SelfSending => Self::SelfSending,
            ShardValidationError::ReceivedSelfPublishedShard => Self::ReceivedSelfPublishedShard,
            ShardValidationError::DuplicateShard => Self::DuplicateShard,
            ShardValidationError::ScheduleManagerError(_) => Self::TreeError,
            ShardValidationError::UnexpectedSender { .. } => Self::ParentVerificationFailed,
            ShardValidationError::SignatureVerificationFailed(_) => {
                Self::SignatureVerificationFailed
            }
            ShardValidationError::MerkleProofVerificationFailed => Self::ProofVerificationFailed,
            ShardValidationError::UnequalShardLengths => Self::UnequalShardLengths,
            ShardValidationError::UnexpectedShardCount { .. } => Self::UnexpectedShardCount,
        }
    }
}

/// Label name for shard send failure reasons
pub const LABEL_NAME_SEND_FAILURE_REASON: &str = "failure_reason";

/// Reasons why shard sending can fail
#[derive(IntoStaticStr, VariantNames)]
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
    ScheduleError,
    /// Committee not registered for the broadcast
    CommitteeNotRegistered,
    /// Broadcast failed to complete
    BroadcastFailed,
}

/// Label name for collection length tracking
pub const LABEL_NAME_COLLECTION: &str = "collection";

/// Collections that are tracked for size/length
#[derive(IntoStaticStr, VariantNames)]
#[strum(serialize_all = "snake_case")]
pub enum CollectionLabel {
    /// Connected peers set
    ConnectedPeers,
    /// Active message processors
    ActiveProcessors,
    /// Number of entries in the finalized messages cache (includes expired entries not yet
    /// evicted)
    FinalizedMessages,
    /// Registered committees
    RegisteredCommittees,
}

/// Metrics for the Propeller protocol.
///
/// Tracks shard throughput, validation rates, message reconstruction, and network health.
pub struct PropellerMetrics {
    /// Total number of shards published (created) by this node
    pub shards_published: MetricCounter,
    /// Total number of shards dispatched to peers (best-effort; actual delivery depends on the
    /// handler)
    pub shards_sent: MetricCounter,
    /// Total number of shard send failures, labeled by reason
    pub shards_send_failed: LabeledMetricCounter,
    /// Total bytes sent in shard data (payload only, excluding protocol overhead)
    /// (best-effort; actual delivery depends on the handler)
    pub shard_bytes_sent: MetricCounter,

    /// Total number of shards received from peers
    pub shards_received: MetricCounter,
    /// Total number of shards that failed validation, labeled by reason
    pub shards_validation_failed: LabeledMetricCounter,
    /// Total number of shard forward operations (one per `SendUnitToPeers` message)
    pub shard_forward_operations: MetricCounter,
    /// Total number of shards forwarded to individual peers (fan-out of forward operations)
    pub shards_forwarded: MetricCounter,
    /// Total bytes received in shard data (payload only, excluding protocol overhead)
    pub shard_bytes_received: MetricCounter,

    /// Total number of messages successfully reconstructed from shards
    pub messages_reconstructed: MetricCounter,
    /// Total number of message reconstruction failures
    pub messages_reconstruction_failed: MetricCounter,
    /// Total number of messages that timed out before completion
    pub messages_timed_out: MetricCounter,

    /// Total number of committee registrations (each registration generates a new tree)
    pub committees_registered: MetricCounter,

    /// Length of various collections (queues, sets, caches) tracked by label
    pub collection_lengths: LabeledMetricGauge,
}

impl PropellerMetrics {
    /// Register all metrics with the metrics system
    pub fn register(&self) {
        self.shards_published.register();
        self.shards_sent.register();
        self.shards_send_failed.register();
        self.shard_bytes_sent.register();

        self.shards_received.register();
        self.shards_validation_failed.register();
        self.shard_forward_operations.register();
        self.shards_forwarded.register();
        self.shard_bytes_received.register();

        self.messages_reconstructed.register();
        self.messages_reconstruction_failed.register();
        self.messages_timed_out.register();

        self.committees_registered.register();

        self.collection_lengths.register();
        // Initialize all labeled metrics to 0 so all label permutations appear in the exporter.
        for variant in ShardValidationFailureReason::VARIANTS {
            self.shards_validation_failed
                .increment(0, &[(LABEL_NAME_VALIDATION_FAILURE_REASON, *variant)]);
        }
        for variant in ShardSendFailureReason::VARIANTS {
            self.shards_send_failed.increment(0, &[(LABEL_NAME_SEND_FAILURE_REASON, *variant)]);
        }
        for variant in CollectionLabel::VARIANTS {
            self.collection_lengths.set(0f64, &[(LABEL_NAME_COLLECTION, *variant)]);
        }
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

    /// Update the length/size of a specific collection
    pub fn update_collection_length(&self, label: CollectionLabel, size: usize) {
        self.collection_lengths.set(size.into_f64(), &[(LABEL_NAME_COLLECTION, label.into())]);
    }

    /// Track metrics based on emitted events
    pub fn track_event(&self, event: &Event) {
        match event {
            Event::MessageReceived { .. } => {
                self.messages_reconstructed.increment(1);
            }
            Event::MessageReconstructionFailed { .. } => {
                self.messages_reconstruction_failed.increment(1);
            }
            Event::ShardValidationFailed { error, .. } => {
                self.increment_validation_failure(error.into());
            }
            Event::ShardSendFailed { error, .. } => {
                let reason = match error {
                    ShardPublishError::LocalPeerNotInPeerWeights => {
                        ShardSendFailureReason::LocalPeerNotInPeerWeights
                    }
                    ShardPublishError::InvalidDataSize => ShardSendFailureReason::InvalidDataSize,
                    ShardPublishError::SigningFailed(_) => ShardSendFailureReason::SigningFailed,
                    ShardPublishError::ErasureEncodingFailed(_) => {
                        ShardSendFailureReason::ErasureEncodingFailed
                    }
                    ShardPublishError::NotConnectedToPeer(_) => {
                        ShardSendFailureReason::NotConnectedToPeer
                    }
                    ShardPublishError::HandlerError(_) => ShardSendFailureReason::HandlerError,
                    ShardPublishError::ScheduleError(_) => ShardSendFailureReason::ScheduleError,
                    ShardPublishError::CommitteeNotRegistered(_) => {
                        ShardSendFailureReason::CommitteeNotRegistered
                    }
                    ShardPublishError::BroadcastFailed => ShardSendFailureReason::BroadcastFailed,
                };
                self.increment_send_failure(reason);
            }
            Event::MessageTimeout { .. } => {
                self.messages_timed_out.increment(1);
            }
        }
    }
}
