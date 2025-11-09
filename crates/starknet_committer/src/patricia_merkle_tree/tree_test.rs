use std::collections::HashMap;

use expect_test::expect;
use rstest::rstest;
use starknet_api::core::{ClassHash, ContractAddress, Nonce, PatriciaKey};
use starknet_api::state::StorageKey;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::filled_tree::node_serde::PatriciaStorageLayout;
use starknet_patricia::patricia_storage::PatriciaStorage;
use starknet_patricia_storage::map_storage::MapStorage;
use starknet_types_core::felt::Felt;
use tracing::level_filters::LevelFilter;

use crate::block_committer::commit::commit_block;
use crate::block_committer::input::{
    ConfigImpl,
    Input,
    StarknetStorageKey,
    StarknetStorageValue,
    StateDiff,
};
use crate::patricia_merkle_tree::tree::fetch_previous_and_new_patricia_paths;
use crate::patricia_merkle_tree::types::{CompiledClassHash, RootHashes};

#[rstest]
#[tokio::test]
async fn test_storage_proofs_different_storage_layouts(
    #[values(PatriciaStorageLayout::Fact, PatriciaStorageLayout::Indexed)]
    storage_layout: PatriciaStorageLayout,
) {
    let mut storage = PatriciaStorage::new(MapStorage::default(), storage_layout);
    let class_hashes = [9u8];
    let contract_addresses = [1u8, 4, 11, 14];

    let storage_updates = [
        (contract_addresses[0], vec![(2u8, 3u8)]),
        (contract_addresses[1], vec![(5u8, 6u8), (7u8, 8u8)]),
    ];
    let class_updates = [(class_hashes[0], 10u8)];
    let address_to_nonce = [(contract_addresses[1], 1u8), (contract_addresses[2], 2u8)];
    let address_to_class_hash = [(contract_addresses[0], 13u8), (contract_addresses[3], 15u8)];

    // Create a state diff with some storage updates.
    let state_diff = StateDiff {
        storage_updates: HashMap::from_iter(storage_updates.into_iter().map(
            |(address, updates)| {
                (
                    ContractAddress(PatriciaKey::try_from(Felt::from(address)).unwrap()),
                    updates
                        .into_iter()
                        .map(|(key, value)| {
                            (
                                StarknetStorageKey(StorageKey(
                                    PatriciaKey::try_from(Felt::from(key)).unwrap(),
                                )),
                                StarknetStorageValue(Felt::from(value)),
                            )
                        })
                        .collect(),
                )
            },
        )),
        class_hash_to_compiled_class_hash: HashMap::from_iter(class_updates.into_iter().map(
            |(class_hash, compiled_class_hash)| {
                (
                    ClassHash(Felt::from(class_hash)),
                    CompiledClassHash(Felt::from(compiled_class_hash)),
                )
            },
        )),
        address_to_nonce: HashMap::from_iter(address_to_nonce.into_iter().map(
            |(address, nonce)| {
                (
                    ContractAddress(PatriciaKey::try_from(Felt::from(address)).unwrap()),
                    Nonce(Felt::from(nonce)),
                )
            },
        )),
        address_to_class_hash: HashMap::from_iter(address_to_class_hash.into_iter().map(
            |(address, class_hash)| {
                (
                    ContractAddress(PatriciaKey::try_from(Felt::from(address)).unwrap()),
                    ClassHash(Felt::from(class_hash)),
                )
            },
        )),
    };

    let input = Input {
        state_diff,
        contracts_trie_root_hash: HashOutput::ROOT_OF_EMPTY_TREE,
        classes_trie_root_hash: HashOutput::ROOT_OF_EMPTY_TREE,
        config: ConfigImpl::new(false, LevelFilter::DEBUG, storage_layout),
    };

    // Commit the state diff.
    let filled_forest = commit_block(input, &mut storage, None).await.unwrap();
    filled_forest.write_to_storage(&mut storage).unwrap();

    // Fetch patricia proofs.
    let contracts_root_hashes = RootHashes {
        previous_root_hash: HashOutput::ROOT_OF_EMPTY_TREE,
        new_root_hash: filled_forest.get_contract_root_hash(),
    };
    let classes_root_hashes = RootHashes {
        previous_root_hash: HashOutput::ROOT_OF_EMPTY_TREE,
        new_root_hash: filled_forest.get_compiled_class_root_hash(),
    };
    let mut proofs = fetch_previous_and_new_patricia_paths(
        &mut storage,
        classes_root_hashes,
        contracts_root_hashes,
        &class_hashes.into_iter().map(|hash| ClassHash(Felt::from(hash))).collect::<Vec<_>>(),
        &contract_addresses
            .into_iter()
            .map(|address| ContractAddress(PatriciaKey::try_from(Felt::from(address)).unwrap()))
            .collect::<Vec<_>>(),
        &HashMap::new(),
    )
    .unwrap();

    // Sort to get deterministic order.
    proofs.classes_trie_proof.sort_keys();
    proofs.contracts_trie_proof.nodes.sort_keys();
    proofs.contracts_trie_proof.leaves.sort_keys();
    proofs.contracts_trie_storage_proofs.sort_keys();
    expect![[r#"
        StarknetForestProofs {
            classes_trie_proof: {
                HashOutput(
                    0x4ddad9824d5445812e4c34ac13b394dc694d2e81ee135dc8e5eae380e6e3b5c,
                ): Edge(
                    EdgeData {
                        bottom_hash: HashOutput(
                            0x145b3dd6889cb1c0315e9395785a3a506cf9be6e1310a5af6a680ddb2f18c74,
                        ),
                        path_to_bottom: PathToBottom {
                            path: EdgePath(
                                9,
                            ),
                            length: EdgePathLength(
                                251,
                            ),
                            _fake_field: (),
                        },
                    },
                ),
            },
            contracts_trie_proof: ContractsTrieProof {
                nodes: {
                    HashOutput(
                        0xf41068516bfaf2f38fe0b687b79d2ba9c16f9e41df3e0b92848d1b8eeaeca6,
                    ): Edge(
                        EdgeData {
                            bottom_hash: HashOutput(
                                0x7877ede3edd20d9671e321742662b820d35d3a9bae3a44072cee129128ec4f2,
                            ),
                            path_to_bottom: PathToBottom {
                                path: EdgePath(
                                    3,
                                ),
                                length: EdgePathLength(
                                    2,
                                ),
                                _fake_field: (),
                            },
                        },
                    ),
                    HashOutput(
                        0x1746983b9266128969bbc52b86da09542f34cba956fae58031cbba3a96ca420,
                    ): Binary(
                        BinaryData {
                            left_hash: HashOutput(
                                0x3fdca528b70e07f541724e92cea3dbd54e01196c34f22e09a5bfa1371334cf4,
                            ),
                            right_hash: HashOutput(
                                0x48a86d10482e663962e9674234fd417d54e483ac7c31c2d500788ff9f2f23ad,
                            ),
                        },
                    ),
                    HashOutput(
                        0x3fdca528b70e07f541724e92cea3dbd54e01196c34f22e09a5bfa1371334cf4,
                    ): Binary(
                        BinaryData {
                            left_hash: HashOutput(
                                0x63863a8eefcb9c98c0e24f8260868b98b840ff7d2592902dd7f4d76d4349b0a,
                            ),
                            right_hash: HashOutput(
                                0x7510b069dbd8c307454383a91a5268668bbc52ce5526b04140c713933488ff9,
                            ),
                        },
                    ),
                    HashOutput(
                        0x48a86d10482e663962e9674234fd417d54e483ac7c31c2d500788ff9f2f23ad,
                    ): Binary(
                        BinaryData {
                            left_hash: HashOutput(
                                0xf41068516bfaf2f38fe0b687b79d2ba9c16f9e41df3e0b92848d1b8eeaeca6,
                            ),
                            right_hash: HashOutput(
                                0x7a180978424ba3fdf8073b0fb4b6032dad7db87ff098d2c564ee5f40bfa7cdd,
                            ),
                        },
                    ),
                    HashOutput(
                        0x4ea370c31f6e3a695aa8107db9f94c72e26601e0cc561582a25643ddbd7f0d7,
                    ): Edge(
                        EdgeData {
                            bottom_hash: HashOutput(
                                0x1746983b9266128969bbc52b86da09542f34cba956fae58031cbba3a96ca420,
                            ),
                            path_to_bottom: PathToBottom {
                                path: EdgePath(
                                    0,
                                ),
                                length: EdgePathLength(
                                    247,
                                ),
                                _fake_field: (),
                            },
                        },
                    ),
                    HashOutput(
                        0x63863a8eefcb9c98c0e24f8260868b98b840ff7d2592902dd7f4d76d4349b0a,
                    ): Edge(
                        EdgeData {
                            bottom_hash: HashOutput(
                                0x7289f3e1d89168df3d95cb5f0f9a4e2d745027304ff623c06881d84332080c9,
                            ),
                            path_to_bottom: PathToBottom {
                                path: EdgePath(
                                    1,
                                ),
                                length: EdgePathLength(
                                    2,
                                ),
                                _fake_field: (),
                            },
                        },
                    ),
                    HashOutput(
                        0x7510b069dbd8c307454383a91a5268668bbc52ce5526b04140c713933488ff9,
                    ): Edge(
                        EdgeData {
                            bottom_hash: HashOutput(
                                0x194569131132358e196e79ce8a40b39e1a6b996f730781be5d102a00dba6e35,
                            ),
                            path_to_bottom: PathToBottom {
                                path: EdgePath(
                                    0,
                                ),
                                length: EdgePathLength(
                                    2,
                                ),
                                _fake_field: (),
                            },
                        },
                    ),
                    HashOutput(
                        0x7a180978424ba3fdf8073b0fb4b6032dad7db87ff098d2c564ee5f40bfa7cdd,
                    ): Edge(
                        EdgeData {
                            bottom_hash: HashOutput(
                                0x4c43843ada8d1fbd50124c3bc373d1e1e3a7270221c316897bff2e7b9669806,
                            ),
                            path_to_bottom: PathToBottom {
                                path: EdgePath(
                                    2,
                                ),
                                length: EdgePathLength(
                                    2,
                                ),
                                _fake_field: (),
                            },
                        },
                    ),
                },
                leaves: {
                    ContractAddress(
                        PatriciaKey(
                            0x1,
                        ),
                    ): ContractState {
                        nonce: Nonce(
                            0x0,
                        ),
                        storage_root_hash: HashOutput(
                            0x14660597f4520490949ad3a519a2aad2e64d445a6660078af3f0fe63de3e366,
                        ),
                        class_hash: ClassHash(
                            0xd,
                        ),
                    },
                    ContractAddress(
                        PatriciaKey(
                            0x4,
                        ),
                    ): ContractState {
                        nonce: Nonce(
                            0x1,
                        ),
                        storage_root_hash: HashOutput(
                            0x10e546e1ab62bf1b232b3cbfcd30e3f0f12f80dc41d4306e15cca0fa9dd9042,
                        ),
                        class_hash: ClassHash(
                            0x0,
                        ),
                    },
                    ContractAddress(
                        PatriciaKey(
                            0xb,
                        ),
                    ): ContractState {
                        nonce: Nonce(
                            0x2,
                        ),
                        storage_root_hash: HashOutput(
                            0x0,
                        ),
                        class_hash: ClassHash(
                            0x0,
                        ),
                    },
                    ContractAddress(
                        PatriciaKey(
                            0xe,
                        ),
                    ): ContractState {
                        nonce: Nonce(
                            0x0,
                        ),
                        storage_root_hash: HashOutput(
                            0x0,
                        ),
                        class_hash: ClassHash(
                            0xf,
                        ),
                    },
                },
            },
            contracts_trie_storage_proofs: {},
        }
    "#]]
    .assert_debug_eq(&proofs);
}
