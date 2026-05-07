use std::collections::HashMap;

use ethnum::U256;
use pretty_assertions::assert_eq;
use rstest::rstest;
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::hash::HashOutput;
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::{
    BinaryData,
    EdgeData,
    EdgePath,
    EdgePathLength,
    PathToBottom,
    Preimage,
    PreimageMap,
};
use starknet_types_core::felt::Felt;

use crate::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use crate::patricia_merkle_tree::types::{ContractsTrieProof, StarknetForestProofs};

fn binary_preimage(node_hash: u128, left: u128, right: u128) -> (HashOutput, Preimage) {
    (
        HashOutput(Felt::from(node_hash)),
        Preimage::Binary(BinaryData {
            left_data: HashOutput(Felt::from(left)),
            right_data: HashOutput(Felt::from(right)),
        }),
    )
}

fn edge_preimage(node_hash: u128, bottom: u128, path: u128, length: u8) -> (HashOutput, Preimage) {
    (
        HashOutput(Felt::from(node_hash)),
        Preimage::Edge(EdgeData {
            bottom_data: HashOutput(Felt::from(bottom)),
            path_to_bottom: PathToBottom::new(
                EdgePath(U256::from(path)),
                EdgePathLength::new(length).unwrap(),
            )
            .unwrap(),
        }),
    )
}

fn contract_state(nonce: u128, storage_root: u128, class_hash: u128) -> ContractState {
    ContractState {
        nonce: Nonce(Felt::from(nonce)),
        storage_root_hash: HashOutput(Felt::from(storage_root)),
        class_hash: ClassHash(Felt::from(class_hash)),
    }
}

fn empty_contracts_trie_proof() -> ContractsTrieProof {
    ContractsTrieProof { nodes: PreimageMap::new(), leaves: HashMap::new() }
}

fn only_classes() -> StarknetForestProofs {
    StarknetForestProofs {
        classes_trie_proof: PreimageMap::from([
            binary_preimage(100, 101, 102),
            edge_preimage(200, 201, 0, 1),
        ]),
        contracts_trie_proof: empty_contracts_trie_proof(),
        contracts_trie_storage_proofs: HashMap::new(),
    }
}

fn only_contracts() -> StarknetForestProofs {
    let address = ContractAddress::try_from(Felt::from(7)).unwrap();
    StarknetForestProofs {
        classes_trie_proof: PreimageMap::new(),
        contracts_trie_proof: ContractsTrieProof {
            nodes: PreimageMap::from([binary_preimage(300, 301, 302)]),
            leaves: HashMap::from([(address, contract_state(1, 400, 500))]),
        },
        contracts_trie_storage_proofs: HashMap::new(),
    }
}

fn classes_and_contracts() -> StarknetForestProofs {
    let address = ContractAddress::try_from(Felt::from(9)).unwrap();
    StarknetForestProofs {
        classes_trie_proof: PreimageMap::from([binary_preimage(110, 111, 112)]),
        contracts_trie_proof: ContractsTrieProof {
            nodes: PreimageMap::from([edge_preimage(310, 311, 1, 1)]),
            leaves: HashMap::from([(address, contract_state(2, 401, 501))]),
        },
        contracts_trie_storage_proofs: HashMap::new(),
    }
}

fn all_leaf_types_single_storage() -> StarknetForestProofs {
    let address = ContractAddress::try_from(Felt::from(11)).unwrap();
    StarknetForestProofs {
        classes_trie_proof: PreimageMap::from([binary_preimage(120, 121, 122)]),
        contracts_trie_proof: ContractsTrieProof {
            nodes: PreimageMap::from([binary_preimage(320, 321, 322)]),
            leaves: HashMap::from([(address, contract_state(3, 402, 502))]),
        },
        contracts_trie_storage_proofs: HashMap::from([(
            address,
            PreimageMap::from([edge_preimage(410, 411, 0, 1)]),
        )]),
    }
}

fn all_leaf_types_multiple_storages() -> StarknetForestProofs {
    let address_a = ContractAddress::try_from(Felt::from(13)).unwrap();
    let address_b = ContractAddress::try_from(Felt::from(14)).unwrap();
    StarknetForestProofs {
        classes_trie_proof: PreimageMap::from([edge_preimage(130, 131, 1, 1)]),
        contracts_trie_proof: ContractsTrieProof {
            nodes: PreimageMap::from([binary_preimage(330, 331, 332)]),
            leaves: HashMap::from([
                (address_a, contract_state(4, 403, 503)),
                (address_b, contract_state(5, 404, 504)),
            ]),
        },
        contracts_trie_storage_proofs: HashMap::from([
            (address_a, PreimageMap::from([binary_preimage(420, 421, 422)])),
            (address_b, PreimageMap::from([edge_preimage(430, 431, 1, 1)])),
        ]),
    }
}

#[rstest]
#[case::only_classes(only_classes())]
#[case::only_contracts(only_contracts())]
#[case::classes_and_contracts(classes_and_contracts())]
#[case::all_leaf_types_single_storage(all_leaf_types_single_storage())]
#[case::all_leaf_types_multiple_storages(all_leaf_types_multiple_storages())]
fn test_starknet_forest_proofs_serialization_round_trip(#[case] proofs: StarknetForestProofs) {
    let encoded = proofs.serialize().unwrap();
    let decoded = StarknetForestProofs::deserialize(&encoded).unwrap();
    assert_eq!(proofs, decoded);
}
