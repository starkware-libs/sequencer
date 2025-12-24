use std::collections::HashSet;
use std::sync::Arc;

use libp2p::identity::PublicKey;
use libp2p::PeerId;

use crate::types::{Channel, ShardSignatureVerificationError};
use crate::{
    signature,
    MessageRoot,
    PropellerScheduleManager,
    PropellerUnit,
    ShardIndex,
    ShardValidationError,
};

#[derive(Debug, Clone)]
pub struct UnitValidator {
    /// the channel the validator belongs to
    channel: Channel,
    /// The publisher of the message.
    publisher: PeerId,
    /// The public key of the publisher.
    publisher_public_key: PublicKey,
    /// The root of the message.
    message_root: MessageRoot,
    /// The signature of the message.
    verified_signature: Option<Vec<u8>>,
    /// The tree manager to use.
    schedule_manager: Arc<PropellerScheduleManager>,
    /// The indices of the received shards.
    received_indices: HashSet<ShardIndex>,
}

impl UnitValidator {
    pub fn new(
        channel: Channel,
        publisher: PeerId,
        publisher_public_key: PublicKey,
        message_root: MessageRoot,
        schedule_manager: Arc<PropellerScheduleManager>,
    ) -> Self {
        Self {
            channel,
            publisher,
            message_root,
            schedule_manager,
            publisher_public_key,
            verified_signature: None,
            received_indices: HashSet::new(),
        }
    }

    /// Verify the signature of a shard using cached metadata.
    ///
    /// This is a performance optimization to avoid verifying the signature if we have already
    /// verified the signature for this message. This optimization is possible because the publisher
    /// signs the merkle root of the message, which is shared by all shards.
    fn verify_signature(
        &mut self,
        unit: &PropellerUnit,
    ) -> Result<(), ShardSignatureVerificationError> {
        if let Some(signature) = &self.verified_signature {
            return if signature == unit.signature() {
                Ok(())
            } else {
                Err(ShardSignatureVerificationError::VerificationFailed)
            };
        }

        let result = signature::verify_message_id_signature(
            &unit.root(),
            unit.signature(),
            &self.publisher_public_key,
        );

        if let Ok(()) = &result {
            self.verified_signature = Some(unit.signature().to_vec());
        }

        result
    }

    pub fn validate_shard(
        &mut self,
        sender: PeerId,
        unit: &PropellerUnit,
    ) -> Result<(), ShardValidationError> {
        // TODO(AndrewL): Think about how to correctly get rid of these assertions
        assert_eq!(self.channel, unit.channel(), "Channel mismatch");
        assert_eq!(self.publisher, unit.publisher(), "Publisher mismatch");
        assert_eq!(self.message_root, unit.root(), "Message root mismatch");

        if self.received_indices.contains(&unit.index()) {
            return Err(ShardValidationError::DuplicateShard);
        }

        self.schedule_manager.validate_origin(sender, unit.publisher(), unit.index())?;
        unit.validate_shard_proof(self.schedule_manager.num_shards())?;
        self.verify_signature(unit).map_err(ShardValidationError::SignatureVerificationFailed)?;

        // add for next time we see this shard
        self.received_indices.insert(unit.index());

        Ok(())
    }
}
