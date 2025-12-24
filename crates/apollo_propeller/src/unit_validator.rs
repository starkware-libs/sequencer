use std::collections::HashSet;
use std::sync::Arc;

use libp2p::identity::PublicKey;
use libp2p::PeerId;

use crate::tree::PropellerTreeManager;
use crate::types::{Channel, ShardSignatureVerificationError};
use crate::{signature, MessageRoot, PropellerUnit, ShardIndex, ShardValidationError};

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
        tree_manager: Arc<PropellerTreeManager>,
    ) -> Self {
        Self {
            channel,
            publisher,
            message_root,
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
        // TODO(AndrewL): Add proof verification
        self.verify_signature(unit).map_err(ShardValidationError::SignatureVerificationFailed)?;

        // add for next time we see this shard
        self.received_indices.insert(unit.index());

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use libp2p::identity::Keypair;

    use super::*;

    fn create_valid_unit(
        channel: Channel,
        publisher: PeerId,
        message_root: MessageRoot,
        index: ShardIndex,
        keypair: &Keypair,
    ) -> PropellerUnit {
        let shard = vec![1, 2, 3];
        let signature = crate::signature::sign_message_id(&message_root, keypair).unwrap();
        PropellerUnit::new(
            channel,
            publisher,
            message_root,
            signature,
            index,
            shard,
            crate::MerkleProof { siblings: vec![] },
        )
    }

    #[test]
    fn test_create_unit_validator() {
        let channel = Channel(1);
        let publisher = PeerId::random();
        let message_root = MessageRoot([1u8; 32]);
        let tree_manager = Arc::new(PropellerTreeManager::new(PeerId::random()));

        let validator = UnitValidator::new(channel, publisher, None, message_root, tree_manager);

        assert_eq!(validator.channel, channel);
        assert_eq!(validator.publisher, publisher);
        assert_eq!(validator.message_root, message_root);
    }

    #[test]
    fn test_duplicate_shard_detection() {
        let channel = Channel(1);
        let keypair = Keypair::generate_ed25519();
        let publisher = PeerId::from(keypair.public());
        let message_root = MessageRoot([1u8; 32]);
        let local_peer = PeerId::random();

        let mut tree_manager = PropellerTreeManager::new(local_peer);
        tree_manager.update_nodes(vec![(local_peer, 100), (publisher, 75)]).unwrap();

        let mut validator = UnitValidator::new(
            channel,
            publisher,
            Some(keypair.public()),
            message_root,
            Arc::new(tree_manager),
        );

        let unit = create_valid_unit(channel, publisher, message_root, ShardIndex(0), &keypair);

        // First validation from publisher (should pass origin check and add index)
        let _result1 = validator.validate_shard(publisher, &unit);

        // Second validation should fail with DuplicateShard
        let result2 = validator.validate_shard(publisher, &unit);
        assert!(matches!(result2, Err(ShardValidationError::DuplicateShard)));
    }

    #[test]
    fn test_origin_verification() {
        let channel = Channel(1);
        let keypair = Keypair::generate_ed25519();
        let publisher = PeerId::from(keypair.public());
        let message_root = MessageRoot([1u8; 32]);
        let local_peer = PeerId::random();
        let wrong_sender = PeerId::random();

        let mut tree_manager = PropellerTreeManager::new(local_peer);
        tree_manager
            .update_nodes(vec![(local_peer, 100), (publisher, 75), (wrong_sender, 50)])
            .unwrap();

        let mut validator = UnitValidator::new(
            channel,
            publisher,
            Some(keypair.public()),
            message_root,
            Arc::new(tree_manager),
        );

        let unit = create_valid_unit(channel, publisher, message_root, ShardIndex(0), &keypair);

        // Validation from wrong sender should fail
        let result = validator.validate_shard(wrong_sender, &unit);
        assert!(matches!(result, Err(ShardValidationError::UnexpectedSender { .. })));

        // Validation from publisher (correct sender for shard 0) should pass
        let result = validator.validate_shard(publisher, &unit);
        assert!(result.is_ok());
    }
}
