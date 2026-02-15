//! Message sharding and reconstruction using erasure coding.
//!
//! This module provides utilities for splitting messages into erasure-coded shards
//! and reconstructing them, including Merkle tree generation and unit preparation.

use libp2p::identity::{Keypair, PeerId};

// Re-export padding functions for backward compatibility
pub use crate::padding::{pad_message, unpad_message};
use crate::reed_solomon::{combine_data_shards, generate_coding_shards, split_data_into_shards};
use crate::types::{Channel, ReconstructionError, ShardIndex, ShardPublishError};
use crate::unit::PropellerUnit;
use crate::{signature, MerkleProof, MerkleTree, MessageRoot};

/// Generate coding shards and create Merkle tree from all shards.
fn finalize_shards(
    data_shards: Vec<Vec<u8>>,
    num_coding_shards: usize,
) -> Result<(Vec<Vec<u8>>, MessageRoot, MerkleTree), ReconstructionError> {
    let coding_shards = generate_coding_shards(&data_shards, num_coding_shards)
        .map_err(ReconstructionError::ErasureReconstructionFailed)?;

    let all_shards = [data_shards, coding_shards].concat();
    let merkle_tree = MerkleTree::new(&all_shards);
    let message_root = MessageRoot(merkle_tree.root().expect("Merkle tree cannot be empty"));

    Ok((all_shards, message_root, merkle_tree))
}

/// Rebuild a message from received shards using erasure coding.
///
/// Returns the reconstructed message, the caller's shard, and the Merkle proof.
pub fn rebuild_message(
    received_shards: Vec<PropellerUnit>,
    message_root: MessageRoot,
    my_shard_index: usize,
    data_count: usize,
    coding_count: usize,
) -> Result<(Vec<u8>, Vec<u8>, MerkleProof), ReconstructionError> {
    let shards_for_reconstruction: Vec<(usize, Vec<u8>)> = received_shards
        .into_iter()
        .map(|mut msg| (msg.index().0.try_into().unwrap(), std::mem::take(msg.shard_mut())))
        .collect();

    let reconstructed_data_shards = crate::reed_solomon::reconstruct_message_from_shards(
        &shards_for_reconstruction,
        data_count,
        coding_count,
    )
    .map_err(ReconstructionError::ErasureReconstructionFailed)?;

    let (mut all_shards, computed_root, merkle_tree) =
        finalize_shards(reconstructed_data_shards.clone(), coding_count)?;

    // Early return if shards have unequal lengths
    let are_all_shards_the_same_length =
        all_shards.iter().all(|shard| shard.len() == all_shards[0].len());
    if !are_all_shards_the_same_length {
        return Err(ReconstructionError::UnequalShardLengths);
    }

    // Early return if message root doesn't match
    if computed_root != message_root {
        return Err(ReconstructionError::MismatchedMessageRoot);
    }

    let message = combine_data_shards(reconstructed_data_shards);
    let un_padded_message = unpad_message(message)?;
    Ok((
        un_padded_message,
        std::mem::take(&mut all_shards[my_shard_index]),
        merkle_tree.prove(my_shard_index).unwrap(),
    ))
}

/// Prepare units for broadcasting.
pub fn prepare_units(
    channel: Channel,
    publisher: PeerId,
    keypair: Keypair,
    message: Vec<u8>,
    num_data_shards: usize,
    num_coding_shards: usize,
) -> Result<Vec<PropellerUnit>, ShardPublishError> {
    // The reed-solomon-simd crate requires the shard length to be even.
    let message = pad_message(message, 2 * num_data_shards);

    let data_shards = split_data_into_shards(message, num_data_shards)
        .ok_or(ShardPublishError::InvalidDataSize)?;

    let (all_shards, message_root, merkle_tree) =
        finalize_shards(data_shards, num_coding_shards)
            .map_err(|_| ShardPublishError::InvalidDataSize)?;

    let signature = signature::sign_message_id(&message_root, &keypair)?;

    let mut messages = Vec::with_capacity(all_shards.len());
    for (index, shard) in all_shards.into_iter().enumerate() {
        let proof = merkle_tree.prove(index).unwrap();
        let message = PropellerUnit::new(
            channel,
            publisher,
            message_root,
            signature.clone(),
            ShardIndex(index.try_into().unwrap()),
            shard,
            proof,
        );
        messages.push(message);
    }

    Ok(messages)
}
