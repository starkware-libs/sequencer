// TODO(AndrewL): Consider re-naming this file.
//! Message sharding and reconstruction using erasure coding.
//!
//! This module provides utilities for splitting messages into erasure-coded shards
//! and reconstructing them, including Merkle tree generation and unit preparation.

use std::num::NonZeroUsize;

use libp2p::identity::{Keypair, PeerId};

use crate::padding::{pad_message, unpad_message};
use crate::reed_solomon::{combine_data_shards, generate_coding_shards, split_data_into_shards};
use crate::types::{Channel, ReconstructionError, ShardIndex, ShardPublishError};
use crate::unit::PropellerUnit;
use crate::{signature, MerkleProof, MerkleTree, MessageRoot};

/// Rebuild a message from received shards using erasure coding.
///
/// Returns the reconstructed message, the caller's shard, and the Merkle proof.
// TODO(AndrewL): Use the fact that ECC is systematic (i.e. data shards are embedded verbatim in
// the encoded output) to avoid re-encoding when all data shards are already present.
// TODO(AndrewL): Wait for more shards to arrive before triggering reconstruction so we can avoid
// the expensive reconstruct path when enough data shards are already present.
// TODO(AndrewL): Consider rebuilding all shards in a single reed-solomon call instead of
// reconstructing data shards and then regenerating coding shards separately.
// <github.com/AndersTrier/reed-solomon-simd/issues/65>
pub fn reconstruct_message_from_shards(
    received_shards: Vec<PropellerUnit>,
    message_root: MessageRoot,
    my_shard_index: usize,
    data_count: usize,
    coding_count: usize,
) -> Result<(Vec<u8>, Vec<u8>, MerkleProof), ReconstructionError> {
    let shards_for_reconstruction: Vec<(usize, Vec<u8>)> = received_shards
        .into_iter()
        .map(|mut msg| {
            (
                msg.index().0.try_into().expect("failed converting u64 ShardIndex to usize"),
                std::mem::take(msg.shard_mut()),
            )
        })
        .collect();

    let reconstructed_data_shards = crate::reed_solomon::reconstruct_message_from_shards(
        &shards_for_reconstruction,
        data_count,
        coding_count,
    )
    .map_err(ReconstructionError::ErasureReconstructionFailed)?;

    let (mut all_shards, computed_root, merkle_tree) =
        create_coding_shards_and_merkle(reconstructed_data_shards.clone(), coding_count)?;

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

    let total_shards = all_shards.len();
    if my_shard_index >= total_shards {
        return Err(ReconstructionError::ErasureReconstructionFailed(format!(
            "my_shard_index {} is out of bounds (total shards: {})",
            my_shard_index, total_shards
        )));
    }

    let message = combine_data_shards(reconstructed_data_shards);
    let un_padded_message =
        unpad_message(message).map_err(ReconstructionError::MessagePaddingError)?;
    Ok((
        un_padded_message,
        std::mem::take(&mut all_shards[my_shard_index]),
        merkle_tree.prove(my_shard_index).expect("my_shard_index was already bounds-checked"),
    ))
}

/// Prepare units for broadcasting.
pub fn create_units_to_publish(
    message: Vec<u8>,
    channel: Channel,
    keypair: Keypair,
    num_data_shards: usize,
    num_coding_shards: usize,
) -> Result<Vec<PropellerUnit>, ShardPublishError> {
    let publisher = PeerId::from(keypair.public());

    // The reed-solomon-simd crate requires the shard length to be even.
    let divisor =
        NonZeroUsize::new(2 * num_data_shards).ok_or(ShardPublishError::InvalidDataSize)?;
    let message = pad_message(message, divisor);

    let data_shards = split_data_into_shards(message, num_data_shards)
        .expect("split_data_into_shards on a padded message should always succeed");

    let (all_shards, message_root, merkle_tree) =
        create_coding_shards_and_merkle(data_shards, num_coding_shards)
            .expect("encoding my own data shards should always succeed");

    let signature = signature::sign_message_id(&message_root, &keypair)?;

    let mut messages = Vec::with_capacity(all_shards.len());
    for (index, shard) in all_shards.into_iter().enumerate() {
        let proof = merkle_tree
            .prove(index)
            .expect("index is within bounds of all_shards from which merkle_tree was built");
        let message = PropellerUnit::new(
            channel,
            publisher,
            message_root,
            signature.clone(),
            ShardIndex(u64::try_from(index).expect("shard index exceeds u64::MAX")),
            shard,
            proof,
        );
        messages.push(message);
    }

    Ok(messages)
}

/// Generate coding shards and create Merkle tree from all shards.
fn create_coding_shards_and_merkle(
    data_shards: Vec<Vec<u8>>,
    num_coding_shards: usize,
) -> Result<(Vec<Vec<u8>>, MessageRoot, MerkleTree), ReconstructionError> {
    let coding_shards = generate_coding_shards(&data_shards, num_coding_shards)
        .map_err(ReconstructionError::ErasureReconstructionFailed)?;

    let all_shards = [data_shards, coding_shards].concat();
    let merkle_tree = MerkleTree::new(&all_shards);
    // TODO(AndrewL): Validate that data_shards is non-empty, or add a default root for an empty
    // merkle tree, instead of panicking here.
    let message_root = MessageRoot(merkle_tree.root().expect("empty merkle tree has no root"));

    Ok((all_shards, message_root, merkle_tree))
}
