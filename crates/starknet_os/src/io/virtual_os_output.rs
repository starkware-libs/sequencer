use cairo_vm::vm::runners::cairo_pie::CairoPie;
use starknet_api::block::BlockNumber;
use starknet_api::hash::StarkHash;
use starknet_api::transaction::fields::VIRTUAL_OS_OUTPUT_VERSION;
use starknet_api::transaction::MessageToL1;
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::{Poseidon, StarkHash as StarkHashTrait};

use crate::io::os_output::{wrap_missing, wrap_missing_as, OsOutputError};

#[cfg(test)]
#[path = "virtual_os_output_test.rs"]
mod virtual_os_output_test;

/// The parsed output of the virtual OS.
#[derive(Debug, PartialEq)]
pub struct VirtualOsOutput {
    /// The output version (should be `VIRTUAL_SNOS0`).
    pub version: Felt,
    /// The base block number.
    pub base_block_number: BlockNumber,
    /// The base block hash.
    pub base_block_hash: StarkHash,
    /// The hash of the Starknet OS config.
    pub starknet_os_config_hash: StarkHash,
    /// Array of hashes, one per message from L2 to L1.
    pub messages_to_l1_hashes: Vec<StarkHash>,
}

impl VirtualOsOutput {
    /// Parses the virtual OS output from a raw output iterator.
    pub fn from_raw_output(raw_output: &[Felt]) -> Result<Self, OsOutputError> {
        let mut iter = raw_output.iter().copied();

        let version = wrap_missing(iter.next(), "version")?;
        let expected_version = VIRTUAL_OS_OUTPUT_VERSION;
        if version != expected_version {
            return Err(OsOutputError::InvalidOsOutputField {
                value_name: "version".to_string(),
                val: version,
                message: format!("expected {expected_version}"),
            });
        }
        let base_block_number = BlockNumber(wrap_missing_as(iter.next(), "base_block_number")?);
        let base_block_hash = wrap_missing(iter.next(), "base_block_hash")?;
        let starknet_os_config_hash = wrap_missing(iter.next(), "starknet_os_config_hash")?;
        let n_messages_to_l1: usize = wrap_missing_as(iter.next(), "n_messages_to_l1")?;

        // Read the hashes array.
        let mut messages_to_l1_hashes = Vec::with_capacity(n_messages_to_l1);
        for i in 0..n_messages_to_l1 {
            let hash = wrap_missing(iter.next(), &format!("messages_to_l1_hashes[{}]", i))?;
            messages_to_l1_hashes.push(hash);
        }

        // Verify that we have consumed all output.
        if iter.next().is_some() {
            return Err(OsOutputError::OutputNotExhausted);
        }

        Ok(Self {
            version,
            base_block_number,
            base_block_hash,
            starknet_os_config_hash,
            messages_to_l1_hashes,
        })
    }
}

/// Computes the Poseidon hash of each message to L1 separately.
/// Each message is serialized as: [from_address, to_address, payload_size, ...payload]
/// Returns an array of hashes, one per message.
pub fn compute_messages_to_l1_hashes(messages: &[MessageToL1]) -> Vec<StarkHash> {
    let mut hashes = Vec::with_capacity(messages.len());
    for message in messages {
        let mut serialized = Vec::new();
        serialized.push(*message.from_address.0.key());
        serialized.push(message.to_address.into());
        serialized.push(Felt::from(message.payload.0.len()));
        serialized.extend_from_slice(&message.payload.0);
        hashes.push(Poseidon::hash_array(&serialized));
    }
    hashes
}

/// The output of the virtual OS runner.
#[derive(Debug)]
pub struct VirtualOsRunnerOutput {
    /// The raw virtual OS output.
    pub raw_output: Vec<Felt>,
    /// The Cairo PIE (Program Independent Execution) artifact.
    pub cairo_pie: CairoPie,
}
