use cairo_vm::vm::runners::cairo_pie::CairoPie;
use starknet_api::block::BlockNumber;
use starknet_api::core::ContractAddress;
use starknet_api::hash::StarkHash;
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
    /// The output version (currently always 0).
    pub version: Felt,
    /// The base block number.
    pub base_block_number: BlockNumber,
    /// The base block hash.
    pub base_block_hash: StarkHash,
    /// The hash of the Starknet OS config.
    pub starknet_os_config_hash: StarkHash,
    /// The address of the authorized account.
    pub authorized_account_address: ContractAddress,
    /// Poseidon hash of messages from L2 to L1.
    pub messages_to_l1_hash: StarkHash,
}

impl VirtualOsOutput {
    /// Parses the virtual OS output from a raw output iterator.
    pub fn from_raw_output(raw_output: &[Felt]) -> Result<Self, OsOutputError> {
        let mut iter = raw_output.iter().copied();

        let version = wrap_missing(iter.next(), "version")?;
        let base_block_number = BlockNumber(wrap_missing_as(iter.next(), "base_block_number")?);
        let base_block_hash = wrap_missing(iter.next(), "base_block_hash")?;
        let starknet_os_config_hash = wrap_missing(iter.next(), "starknet_os_config_hash")?;
        let authorized_account_address =
            wrap_missing_as(iter.next(), "authorized_account_address")?;
        let messages_to_l1_hash = wrap_missing(iter.next(), "messages_to_l1_hash")?;

        // Verify that we have consumed all output.
        if iter.next().is_some() {
            return Err(OsOutputError::OutputNotExhausted);
        }

        Ok(Self {
            version,
            base_block_number,
            base_block_hash,
            starknet_os_config_hash,
            authorized_account_address,
            messages_to_l1_hash,
        })
    }
}

/// Computes the Poseidon hash of messages to L1 in the same format as Cairo outputs them.
/// Each message is serialized as: [from_address, to_address, payload_size, ...payload]
/// Messages are concatenated with no separators.
pub fn compute_messages_to_l1_hash(messages: &[MessageToL1]) -> StarkHash {
    let mut serialized = Vec::new();
    for message in messages {
        serialized.push(*message.from_address.0.key());
        serialized.push(message.to_address.into());
        serialized.push(Felt::from(message.payload.0.len()));
        serialized.extend_from_slice(&message.payload.0);
    }
    Poseidon::hash_array(&serialized)
}

/// The output of the virtual OS runner.
#[derive(Debug)]
pub struct VirtualOsRunnerOutput {
    /// The raw virtual OS output.
    pub raw_output: Vec<Felt>,
    /// The Cairo PIE (Program Independent Execution) artifact.
    pub cairo_pie: CairoPie,
}
