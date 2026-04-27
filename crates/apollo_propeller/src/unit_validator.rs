use std::collections::HashSet;
use std::sync::Arc;

use libp2p::identity::PublicKey;
use libp2p::PeerId;

use crate::types::{CommitteeId, SignatureVerificationError, VerifiedFields};
use crate::{
    signature,
    MessageRoot,
    PropellerScheduleManager,
    PropellerUnit,
    UnitIndex,
    UnitValidationError,
};

#[derive(Debug, Clone)]
pub struct UnitValidator {
    /// the committee the validator belongs to
    committee_id: CommitteeId,
    /// The publisher of the message.
    publisher: PeerId,
    /// The public key of the publisher.
    publisher_public_key: PublicKey,
    /// The root of the message.
    message_root: MessageRoot,
    /// Cached signature and timestamp after first successful verification.
    verified_fields: Option<VerifiedFields>,
    /// The tree manager to use.
    schedule_manager: Arc<PropellerScheduleManager>,
    /// The indices of the received units.
    received_indices: HashSet<UnitIndex>,
}

impl UnitValidator {
    pub fn new(
        committee_id: CommitteeId,
        publisher: PeerId,
        publisher_public_key: PublicKey,
        message_root: MessageRoot,
        schedule_manager: Arc<PropellerScheduleManager>,
    ) -> Self {
        Self {
            committee_id,
            publisher,
            message_root,
            schedule_manager,
            publisher_public_key,
            verified_fields: None,
            received_indices: HashSet::new(),
        }
    }

    /// Verify the signature of a unit using cached metadata.
    ///
    /// This is a performance optimization to avoid verifying the signature if we have already
    /// verified the signature for this message. This optimization is possible because the publisher
    /// signs the merkle root of the message, which is shared by all units.
    fn verify_signature(&mut self, unit: &PropellerUnit) -> Result<(), SignatureVerificationError> {
        if let Some(verified_fields) = &self.verified_fields {
            let VerifiedFields { signature, nonce } = verified_fields;
            return if signature == unit.signature() && *nonce == unit.nonce() {
                Ok(())
            } else {
                Err(SignatureVerificationError::VerificationFailed)
            };
        }

        let result = signature::verify_message_id_signature(
            &unit.root(),
            self.committee_id,
            unit.nonce(),
            unit.signature(),
            &self.publisher_public_key,
        );

        if let Ok(()) = &result {
            self.verified_fields =
                Some(VerifiedFields { signature: unit.signature().to_vec(), nonce: unit.nonce() });
        }

        result
    }

    pub fn validate_unit(
        &mut self,
        sender: PeerId,
        unit: &PropellerUnit,
    ) -> Result<(), UnitValidationError> {
        // TODO(AndrewL): Think about how to correctly get rid of these assertions
        assert_eq!(self.committee_id, unit.committee_id(), "Committee mismatch");
        assert_eq!(self.publisher, unit.publisher(), "Publisher mismatch");
        assert_eq!(self.message_root, unit.root(), "Message root mismatch");

        if self.received_indices.contains(&unit.index()) {
            return Err(UnitValidationError::DuplicateUnit);
        }

        self.schedule_manager.validate_origin(sender, unit.publisher(), unit.index())?;
        // TODO(AndrewL): Replace the hardcoded 1 with a configurable shards-per-peer count
        // once reconstruction supports multiple shards per peer.
        unit.validate_shard_count(1)?;
        unit.validate_shard_lengths()?;
        unit.validate_merkle_proof(self.schedule_manager.num_units())?;
        self.verify_signature(unit).map_err(UnitValidationError::SignatureVerificationFailed)?;

        // add for next time we see this unit's index
        self.received_indices.insert(unit.index());

        Ok(())
    }
}
