// TODO(AndrewL): Consider re-naming this file.
//! Message sharding and reconstruction using erasure coding.
//!
//! This module provides utilities for splitting messages into erasure-coded shards
//! and reconstructing them, including Merkle tree generation and unit preparation.

use std::num::NonZeroUsize;
use std::time::{SystemTime, UNIX_EPOCH};

use libp2p::identity::{Keypair, PeerId};

use crate::padding::{pad_message, unpad_message};
use crate::reed_solomon::{combine_data_shards, generate_coding_shards, split_data_into_shards};
use crate::types::{CommitteeId, ReconstructionError, ShardIndex, UnitPublishError};
use crate::unit::{PropellerUnit, Shard, ShardsOfPeer};
use crate::{signature, MerkleProof, MerkleTree, MessageRoot};

/// Rebuild a message from received units using erasure coding.
///
/// Returns the reconstructed message, the caller's shards, and the Merkle proof.
// TODO(AndrewL): Use the fact that ECC is systematic (i.e. data shards are embedded verbatim in
// the encoded output) to avoid re-encoding when all data shards are already present.
// TODO(AndrewL): Wait for more shards to arrive before triggering reconstruction so we can avoid
// the expensive reconstruct path when enough data shards are already present.
// TODO(AndrewL): Consider rebuilding all shards in a single reed-solomon call instead of
// reconstructing data shards and then regenerating coding shards separately.
// <github.com/AndersTrier/reed-solomon-simd/issues/65>
pub fn reconstruct_data_shards(
    received_units: Vec<PropellerUnit>,
    message_root: MessageRoot,
    my_shard_index: usize,
    data_count: usize,
    coding_count: usize,
) -> Result<(Vec<u8>, ShardsOfPeer, MerkleProof), ReconstructionError> {
    let shards_for_reconstruction: Vec<(usize, Vec<u8>)> = received_units
        .into_iter()
        .map(|mut msg| {
            let index: usize =
                msg.index().0.try_into().expect("failed converting u64 ShardIndex to usize");
            // TODO(AndrewL): Support multiple shards per peer once reconstruction handles it.
            let [shard] = <[Shard; 1]>::try_from(std::mem::take(&mut msg.shards_mut().0)).map_err(
                |shards| ReconstructionError::UnexpectedShardCount {
                    expected: 1,
                    actual: shards.len(),
                },
            )?;
            Ok((index, shard.0))
        })
        .collect::<Result<_, _>>()?;

    let reconstructed_data_shards = crate::reed_solomon::reconstruct_data_shards(
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
    // TODO(AndrewL): Support multiple shards per peer once reconstruction handles it.
    let my_shards = ShardsOfPeer(vec![Shard(std::mem::take(&mut all_shards[my_shard_index]))]);
    Ok((
        un_padded_message,
        my_shards,
        merkle_tree.prove(my_shard_index).expect("my_shard_index was already bounds-checked"),
    ))
}

/// Prepare units for broadcasting.
pub fn create_units_to_publish(
    message: Vec<u8>,
    committee_id: CommitteeId,
    keypair: Keypair,
    num_data_shards: usize,
    num_coding_shards: usize,
) -> Result<Vec<PropellerUnit>, UnitPublishError> {
    let publisher = PeerId::from(keypair.public());

    // The reed-solomon-simd crate requires the shard length to be even.
    let divisor =
        NonZeroUsize::new(2 * num_data_shards).ok_or(UnitPublishError::InvalidDataSize)?;
    let message = pad_message(message, divisor);

    let data_shards = split_data_into_shards(message, num_data_shards)
        .expect("split_data_into_shards on a padded message should always succeed");

    let (all_shards, message_root, merkle_tree) =
        create_coding_shards_and_merkle(data_shards, num_coding_shards)
            .expect("encoding my own data shards should always succeed");

    let timestamp_ns =
        SystemTime::now().duration_since(UNIX_EPOCH).expect("system clock is set").as_nanos();
    let nonce = u64::try_from(timestamp_ns)
        .expect("timestamp in nanos since UNIX_EPOCH should fit in u64, until year 2554");
    let signature = signature::sign_message_id(&message_root, committee_id, nonce, &keypair)?;
    let mut messages = Vec::with_capacity(all_shards.len());

    for (index, shard) in all_shards.into_iter().enumerate() {
        let proof = merkle_tree
            .prove(index)
            .expect("index is within bounds of all_shards from which merkle_tree was built");
        let message = PropellerUnit::new(
            committee_id,
            publisher,
            message_root,
            signature.clone(),
            ShardIndex(u64::try_from(index).expect("shard index exceeds u64::MAX")),
            // TODO(AndrewL): Support multiple shards per peer once reconstruction handles it.
            ShardsOfPeer(vec![Shard(shard)]),
            proof,
            nonce,
        );
        messages.push(message);
    }

    Ok(messages)
}

/// Generate coding shards and create Merkle tree from all shards.
///
/// Each shard is wrapped in a `ShardsOfPeer` proto message and the Merkle leaf is the
/// proto-encoded bytes of that message, ensuring cross-language determinism.
fn create_coding_shards_and_merkle(
    data_shards: Vec<Vec<u8>>,
    num_coding_shards: usize,
) -> Result<(Vec<Vec<u8>>, MessageRoot, MerkleTree), ReconstructionError> {
    let coding_shards = generate_coding_shards(&data_shards, num_coding_shards)
        .map_err(ReconstructionError::ErasureReconstructionFailed)?;

    let all_shards = [data_shards, coding_shards].concat();
    let leaf_data: Vec<Vec<u8>> = all_shards
        .iter()
        .map(|shard| ShardsOfPeer(vec![Shard(shard.clone())]).encode_to_proto_bytes())
        .collect();
    let merkle_tree = MerkleTree::new(&leaf_data);
    // TODO(AndrewL): Validate that data_shards is non-empty, or add a default root for an empty
    // merkle tree, instead of panicking here.
    let message_root = MessageRoot(merkle_tree.root().expect("empty merkle tree has no root"));

    Ok((all_shards, message_root, merkle_tree))
}
