use std::cmp::min;
use std::collections::HashMap;

use ethnum::U256;
use rand::prelude::IteratorRandom;
use rand::Rng;
use rand_distr::num_traits::ToPrimitive;
use rand_distr::{Distribution, Geometric};
use starknet_api::core::{ClassHash, ContractAddress, Nonce, PATRICIA_KEY_UPPER_BOUND};
use starknet_patricia::felt::u256_from_felt;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::external_test_utils::{
    get_random_u256,
    u256_try_into_felt,
};
use starknet_patricia::patricia_merkle_tree::filled_tree::node::FilledNode;
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::{
    BinaryData,
    EdgeData,
    EdgePath,
    EdgePathLength,
    NodeData,
    NodeDataDiscriminants as NodeDataVariants,
    PathToBottom,
};
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_types_core::felt::Felt;
use strum::IntoEnumIterator;

use crate::block_committer::input::StarknetStorageValue;
use crate::forest::filled_forest::FilledForest;
use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::types::{
    ClassesTrie,
    CompiledClassHash,
    ContractsTrie,
    StorageTrie,
    StorageTrieMap,
};

pub trait RandomValue {
    fn random<R: Rng>(rng: &mut R, max: Option<U256>) -> Self;
}

pub trait DummyRandomValue {
    fn dummy_random<R: Rng>(rng: &mut R, max: Option<U256>) -> Self;
}

impl RandomValue for Felt {
    fn random<R: Rng>(rng: &mut R, _max: Option<U256>) -> Self {
        u256_try_into_felt(&get_random_u256(rng, U256::ONE, u256_from_felt(&Felt::MAX) + 1))
            .expect("Failed to create a random Felt")
    }
}

impl RandomValue for HashOutput {
    fn random<R: Rng>(rng: &mut R, max: Option<U256>) -> Self {
        HashOutput(Felt::random(rng, max))
    }
}

impl RandomValue for StarknetStorageValue {
    fn random<R: Rng>(rng: &mut R, max: Option<U256>) -> Self {
        StarknetStorageValue(Felt::random(rng, max))
    }
}

impl RandomValue for CompiledClassHash {
    fn random<R: Rng>(rng: &mut R, max: Option<U256>) -> Self {
        CompiledClassHash(Felt::random(rng, max))
    }
}

impl RandomValue for ContractState {
    fn random<R: Rng>(rng: &mut R, max: Option<U256>) -> Self {
        ContractState {
            nonce: Nonce(Felt::random(rng, max)),
            storage_root_hash: HashOutput::random(rng, max),
            class_hash: ClassHash(Felt::random(rng, max)),
        }
    }
}

impl RandomValue for BinaryData {
    fn random<R: Rng>(rng: &mut R, max: Option<U256>) -> Self {
        Self { left_hash: HashOutput::random(rng, max), right_hash: HashOutput::random(rng, max) }
    }
}

impl RandomValue for PathToBottom {
    fn random<R: Rng>(rng: &mut R, max: Option<U256>) -> Self {
        // Crate a random path and than calculate the length of the path.
        let path = EdgePath::random(rng, max);
        // TODO(Aviv, 27/6/2024): use a built in function once we migrate to a better big-integer
        // library Randomly choose the number of real leading zeros in the path (up to the
        // maximum possible). Real leading zero is a zero that refer to a left node, and not
        // a padding zero.
        let max_real_leading_zeros = path.0.leading_zeros() - EdgePath::MAX.0.leading_zeros();
        let real_leading_zeros = std::cmp::min(
            Geometric::new(0.5)
                .expect("Failed to create random variable.")
                .sample(rng)
                .to_u32()
                .expect("failed to cast random variable to u32"),
            max_real_leading_zeros,
        );
        let length: u8 = (256_u32 - path.0.leading_zeros() + real_leading_zeros)
            .try_into()
            .expect("Leading zeros conversion to u8 failed");

        Self::new(path, EdgePathLength::new(length).expect("Invalid length"))
            .expect("Illegal PathToBottom")
    }
}

impl RandomValue for EdgePath {
    fn random<R: Rng>(rng: &mut R, max: Option<U256>) -> Self {
        // The maximum value is the maximum between max and EdgePath::MAX.
        let max_value = match max {
            Some(m) => min(m, EdgePath::MAX.0),
            None => EdgePath::MAX.0,
        };

        Self(get_random_u256(rng, U256::ONE, max_value + 1))
    }
}

impl RandomValue for EdgeData {
    fn random<R: Rng>(rng: &mut R, max: Option<U256>) -> Self {
        Self {
            bottom_hash: HashOutput::random(rng, max),
            path_to_bottom: PathToBottom::random(rng, max),
        }
    }
}

macro_rules! random_node_data {
    ($leaf:ty) => {
        impl RandomValue for NodeData<$leaf> {
            fn random<R: Rng>(rng: &mut R, max: Option<U256>) -> Self {
                match NodeDataVariants::iter()
                    .choose(rng)
                    .expect("Failed to choose a random variant for NodeData")
                {
                    NodeDataVariants::Binary => NodeData::Binary(BinaryData::random(rng, max)),
                    NodeDataVariants::Edge => NodeData::Edge(EdgeData::random(rng, max)),
                    NodeDataVariants::Leaf => NodeData::Leaf(<$leaf>::random(rng, max)),
                }
            }
        }
    };
}

random_node_data!(StarknetStorageValue);
random_node_data!(CompiledClassHash);
random_node_data!(ContractState);

impl RandomValue for NodeIndex {
    fn random<R: Rng>(rng: &mut R, max: Option<U256>) -> Self {
        // The maximum value is the maximum between max and NodeIndex::MAX.
        let max_value = match max {
            Some(m) => min(m, U256::from(NodeIndex::MAX)),
            None => U256::from(NodeIndex::MAX),
        };

        Self::new(get_random_u256(rng, U256::ONE, max_value + 1))
    }
}

macro_rules! random_filled_node {
    ($leaf:ty) => {
        impl RandomValue for FilledNode<$leaf> {
            fn random<R: Rng>(rng: &mut R, max: Option<U256>) -> Self {
                Self { data: NodeData::random(rng, max), hash: HashOutput::random(rng, max) }
            }
        }
    };
}

random_filled_node!(StarknetStorageValue);
random_filled_node!(CompiledClassHash);
random_filled_node!(ContractState);

impl RandomValue for ContractAddress {
    fn random<R: Rng>(rng: &mut R, max: Option<U256>) -> Self {
        let address_max = u256_from_felt(&Felt::from_hex_unchecked(PATRICIA_KEY_UPPER_BOUND));
        let max = match max {
            None => address_max,
            Some(caller_max) => min(address_max, caller_max),
        };
        ContractAddress::try_from(Felt::random(rng, Some(max))).unwrap()
    }
}

macro_rules! random_filled_tree {
    ($tree:ty, $leaf:ty) => {
        impl DummyRandomValue for $tree {
            fn dummy_random<R: Rng>(rng: &mut R, max_size: Option<U256>) -> Self {
                // The maximum node number is the maximum between max and 101.
                let max_node_number = match max_size {
                    Some(m) => m,
                    None => U256::from(101_u8),
                }
                .as_usize();

                let mut nodes: Vec<(NodeIndex, FilledNode<$leaf>)> = (0..max_node_number)
                    .map(|_| (NodeIndex::random(rng, max_size), FilledNode::random(rng, max_size)))
                    .collect();

                nodes.push((NodeIndex::ROOT, FilledNode::random(rng, max_size)));

                Self {
                    tree_map: nodes.into_iter().collect(),
                    root_hash: HashOutput(Felt::random(rng, max_size)),
                }
            }
        }
    };
}

random_filled_tree!(StorageTrie, StarknetStorageValue);
random_filled_tree!(ClassesTrie, CompiledClassHash);
random_filled_tree!(ContractsTrie, ContractState);

impl DummyRandomValue for FilledForest {
    /// Generates a dummy random filled forest.
    /// The forest contains max(m,98) dummy random storage tries,
    /// a dummy random contract tree and a dummy random compiled class tree.
    /// Does not necessary represent a valid forest.
    fn dummy_random<R: Rng>(rng: &mut R, max_size: Option<U256>) -> Self {
        // The maximum storage tries number is the maximum between max and 98.
        // We also use this number to be the maximum tree size,
        let max_trees_number = match max_size {
            Some(m) => m,
            None => U256::from(98_u8),
        }
        .as_usize();

        let storage_tries: StorageTrieMap = (0..max_trees_number)
            .map(|_| {
                (ContractAddress::random(rng, max_size), StorageTrie::dummy_random(rng, max_size))
            })
            .collect::<HashMap<_, _>>();

        let contracts_trie = ContractsTrie::dummy_random(rng, max_size);
        let classes_trie = ClassesTrie::dummy_random(rng, max_size);

        Self { storage_tries, contracts_trie, classes_trie }
    }
}
