use std::collections::HashSet;
use std::sync::Arc;

use libp2p::identity::PublicKey;
use libp2p::PeerId;

use crate::tree::PropellerTreeManager;
use crate::types::{Channel, ShardSignatureVerificationError};
use crate::{
    signature,
    MessageRoot,
    PropellerUnit,
    ShardIndex,
    ShardValidationError,
    ValidationMode,
};

#[derive(Debug, Clone)]
pub struct UnitValidator {
    /// the channel the validator belongs to
    channel: Channel,
    /// The publisher of the message.
    publisher: PeerId,
    /// The public key of the publisher.
    publisher_public_key: Option<PublicKey>,
    /// The root of the message.
    message_root: MessageRoot,
    /// The signature of the message.
    signature: Option<Vec<u8>>,
    /// The validation mode to use.
    validation_mode: ValidationMode,
    /// The tree manager to use.
    tree_manager: Arc<PropellerTreeManager>,
    /// The indices of the received shards.
    received_indices: HashSet<ShardIndex>,
}

impl UnitValidator {
    pub fn new(
        channel: Channel,
        publisher: PeerId,
        publisher_public_key: Option<PublicKey>,
        message_root: MessageRoot,
        validation_mode: ValidationMode,
        tree_manager: Arc<PropellerTreeManager>,
    ) -> Self {
        Self {
            channel,
            publisher,
            message_root,
            validation_mode,
            tree_manager,
            publisher_public_key,
            signature: None,
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
        if self.validation_mode == ValidationMode::None {
            return Ok(());
        }

        if let Some(signature) = &self.signature {
            return if signature == unit.signature() {
                Ok(())
            } else {
                Err(ShardSignatureVerificationError::VerificationFailed)
            };
        }

        let public_key = self
            .publisher_public_key
            .as_ref()
            .ok_or(ShardSignatureVerificationError::NoPublicKeyAvailable(self.publisher))?;

        let r = signature::verify_message_id_signature(&unit.root(), unit.signature(), public_key);

        if let Ok(()) = &r {
            self.signature = Some(unit.signature().to_vec());
        }

        r
    }

    pub fn validate_shard(
        &mut self,
        sender: PeerId,
        unit: &PropellerUnit,
    ) -> Result<(), ShardValidationError> {
        assert_eq!(self.channel, unit.channel(), "Channel mismatch");
        assert_eq!(self.publisher, unit.publisher(), "Publisher mismatch");
        assert_eq!(self.message_root, unit.root(), "Message root mismatch");

        if self.received_indices.contains(&unit.index()) {
            return Err(ShardValidationError::DuplicateShard);
        }

        self.tree_manager.validate_origin(sender, unit)?;
        unit.validate_shard_proof()?;
        self.verify_signature(unit).map_err(ShardValidationError::SignatureVerificationFailed)?;

        // add for next time we see this shard
        self.received_indices.insert(unit.index());

        Ok(())
    }
}
