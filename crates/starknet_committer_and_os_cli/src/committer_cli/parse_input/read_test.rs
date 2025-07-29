use std::collections::HashMap;

use assert_matches::assert_matches;
use pretty_assertions::assert_eq;
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_committer::block_committer::input::{
    ConfigImpl,
    Input,
    StarknetStorageKey,
    StarknetStorageValue,
    StateDiff,
};
use starknet_committer::patricia_merkle_tree::types::CompiledClassHash;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia_storage::errors::DeserializationError;
use starknet_patricia_storage::storage_trait::{DbKey, DbValue};
use starknet_types_core::felt::Felt;
use tracing::level_filters::LevelFilter;

use super::parse_input;
use crate::committer_cli::parse_input::cast::CommitterInputImpl;

#[test]
fn test_simple_input_parsing() {
    let input = r#"
[
    [
        [
            [14,6,78,90],
            [245,90,0,0,1]
        ],
        [
            [14,6,43,90],
            [9,0,0,0,1]
        ]
    ],
    [
        [
            [
                [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 1, 0, 89, 0, 0, 0, 0, 0, 0, 0],
                [0, 0, 0, 0, 0, 0, 0, 0, 0, 45, 77, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
            ],
            [
                [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 11, 5, 0, 0, 0, 0, 0, 1, 0, 89, 0, 0, 0, 0, 0, 0, 0],
                [0, 0, 0, 0, 0, 0, 0, 0, 0, 45, 77, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
            ]
        ],
        [

            [
                [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 1, 0, 89, 0, 0, 0, 0, 0, 0, 0],
                [0, 0, 0, 0, 0, 14, 0, 0, 0, 45, 77, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
            ],
            [
                [0, 0, 0, 0, 0, 98, 0, 0, 0, 0, 0, 0, 0, 0, 0, 11, 5, 0, 0, 0, 0, 0, 1, 0, 89, 0, 0, 0, 0, 0, 0, 0],
                [0, 0, 0, 0, 0, 0, 0, 0, 0, 45, 77, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
            ]

        ],
        [
            [
                [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 1, 0, 89, 0, 0, 0, 0, 0, 0, 0],
                [0, 0, 0, 0, 0, 14, 0, 0, 0, 45, 77, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
            ],
            [
                [0, 0, 0, 0, 0, 98, 0, 0, 0, 156, 0, 0, 0, 0, 0, 11, 5, 0, 0, 0, 0, 0, 1, 0, 89, 0, 0, 0, 0, 0, 0, 0],
                [0, 0, 0, 0, 0, 0, 0, 0, 0, 45, 77, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
            ]
        ],
        [
            [
                [0, 0, 0, 0, 0, 98, 0, 0, 0, 156, 0, 0, 0, 0, 0, 11, 5, 0, 0, 0, 0, 0, 1, 0, 89, 0, 0, 0, 0, 0, 0, 0],
                [
                    [
                        [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 1, 0, 89, 0, 0, 0, 0, 0, 0, 0],
                        [0, 0, 0, 0, 0, 14, 0, 0, 0, 45, 77, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
                    ],
                    [
                        [0, 0, 0, 0, 0, 98, 0, 0, 0, 156, 0, 0, 0, 0, 0, 11, 5, 0, 0, 0, 0, 0, 1, 0, 89, 0, 0, 0, 0, 0, 0, 0],
                        [0, 0, 0, 0, 0, 0, 0, 0, 0, 45, 77, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
                    ]
                ]
            ]
        ]
    ],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 19],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0],
    {"warn_on_trivial_modifications": true, "log_level": 5}
]

"#;
    let expected_storage = HashMap::from([
        (DbKey([14, 6, 78, 90].to_vec()), DbValue([245, 90, 0, 0, 1].to_vec())),
        (DbKey([14, 6, 43, 90].to_vec()), DbValue([9, 0, 0, 0, 1].to_vec())),
    ]);

    let expected_address_to_class_hash = HashMap::from([
        (
            ContractAddress::try_from(Felt::from_bytes_be_slice(&[
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 1, 0, 89, 0, 0,
                0, 0, 0, 0, 0,
            ]))
            .unwrap(),
            ClassHash(Felt::from_bytes_be_slice(&[
                0, 0, 0, 0, 0, 0, 0, 0, 0, 45, 77, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0,
            ])),
        ),
        (
            ContractAddress::try_from(Felt::from_bytes_be_slice(&[
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 11, 5, 0, 0, 0, 0, 0, 1, 0, 89, 0, 0,
                0, 0, 0, 0, 0,
            ]))
            .unwrap(),
            ClassHash(Felt::from_bytes_be_slice(&[
                0, 0, 0, 0, 0, 0, 0, 0, 0, 45, 77, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0,
            ])),
        ),
    ]);

    let expected_address_to_nonce = HashMap::from([
        (
            ContractAddress::try_from(Felt::from_bytes_be_slice(&[
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 1, 0, 89, 0, 0,
                0, 0, 0, 0, 0,
            ]))
            .unwrap(),
            Nonce(Felt::from_bytes_be_slice(&[
                0, 0, 0, 0, 0, 14, 0, 0, 0, 45, 77, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0,
            ])),
        ),
        (
            ContractAddress::try_from(Felt::from_bytes_be_slice(&[
                0, 0, 0, 0, 0, 98, 0, 0, 0, 0, 0, 0, 0, 0, 0, 11, 5, 0, 0, 0, 0, 0, 1, 0, 89, 0, 0,
                0, 0, 0, 0, 0,
            ]))
            .unwrap(),
            Nonce(Felt::from_bytes_be_slice(&[
                0, 0, 0, 0, 0, 0, 0, 0, 0, 45, 77, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0,
            ])),
        ),
    ]);

    let expected_class_hash_to_compiled_class_hash = HashMap::from([
        (
            ClassHash(Felt::from_bytes_be_slice(&[
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 1, 0, 89, 0, 0,
                0, 0, 0, 0, 0,
            ])),
            CompiledClassHash(Felt::from_bytes_be_slice(&[
                0, 0, 0, 0, 0, 14, 0, 0, 0, 45, 77, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0,
            ])),
        ),
        (
            ClassHash(Felt::from_bytes_be_slice(&[
                0, 0, 0, 0, 0, 98, 0, 0, 0, 156, 0, 0, 0, 0, 0, 11, 5, 0, 0, 0, 0, 0, 1, 0, 89, 0,
                0, 0, 0, 0, 0, 0,
            ])),
            CompiledClassHash(Felt::from_bytes_be_slice(&[
                0, 0, 0, 0, 0, 0, 0, 0, 0, 45, 77, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0,
            ])),
        ),
    ]);

    let expected_storage_updates = HashMap::from([(
        ContractAddress::try_from(Felt::from_bytes_be_slice(&[
            0, 0, 0, 0, 0, 98, 0, 0, 0, 156, 0, 0, 0, 0, 0, 11, 5, 0, 0, 0, 0, 0, 1, 0, 89, 0, 0,
            0, 0, 0, 0, 0,
        ]))
        .unwrap(),
        HashMap::from([
            (
                StarknetStorageKey(
                    Felt::from_bytes_be_slice(&[
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 1, 0, 89,
                        0, 0, 0, 0, 0, 0, 0,
                    ])
                    .try_into()
                    .unwrap(),
                ),
                StarknetStorageValue(Felt::from_bytes_be_slice(&[
                    0, 0, 0, 0, 0, 14, 0, 0, 0, 45, 77, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0,
                ])),
            ),
            (
                StarknetStorageKey(
                    Felt::from_bytes_be_slice(&[
                        0, 0, 0, 0, 0, 98, 0, 0, 0, 156, 0, 0, 0, 0, 0, 11, 5, 0, 0, 0, 0, 0, 1, 0,
                        89, 0, 0, 0, 0, 0, 0, 0,
                    ])
                    .try_into()
                    .unwrap(),
                ),
                StarknetStorageValue(Felt::from_bytes_be_slice(&[
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 45, 77, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0,
                ])),
            ),
        ]),
    )]);

    let expected_contracts_trie_root_hash = HashOutput(Felt::from(19_u128));
    let expected_classes_trie_root_hash = HashOutput(Felt::from(256_u128));
    let expected_input = Input {
        state_diff: StateDiff {
            address_to_class_hash: expected_address_to_class_hash,
            address_to_nonce: expected_address_to_nonce,
            class_hash_to_compiled_class_hash: expected_class_hash_to_compiled_class_hash,
            storage_updates: expected_storage_updates,
        },
        contracts_trie_root_hash: expected_contracts_trie_root_hash,
        classes_trie_root_hash: expected_classes_trie_root_hash,
        config: ConfigImpl::new(true, LevelFilter::DEBUG),
    };
    assert_eq!(
        parse_input(input).unwrap(),
        CommitterInputImpl { input: expected_input, storage: expected_storage }
    );
}

#[test]
fn test_input_parsing_with_storage_key_duplicate() {
    let input = r#"
[
    [
        [
            [14,6,78,90],
            [245,90,0,0,1]
        ],
        [
            [14,6,78,90],
            [9,0,0,0,1]
        ]
    ],
    [
        [],
        [],
        [],
        []
    ],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 17, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 222, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 9, 0, 0, 0, 0, 0, 3],
    {"warn_on_trivial_modifications": true, "log_level": 20}
]

"#;
    let expected_error = "storage: DbKey([14, 6, 78, 90])";
    assert!(matches!(
        parse_input(input).unwrap_err(),
        DeserializationError::KeyDuplicate(key) if key == expected_error
    ));
}

#[test]
fn test_input_parsing_with_mapping_key_duplicate() {
    let input = r#"
[
    [
        [
            [14,6,78,90],
            [245,90,0,0,1]
        ],
        [
            [0,6,0,90],
            [9,0,0,0,1]
        ]
    ],
    [
        [
            [
                [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 1, 0, 89, 0, 0, 0, 0, 0, 0, 0],
                [0, 0, 0, 0, 0, 0, 0, 0, 0, 45, 77, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
            ],
            [
                [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 1, 0, 89, 0, 0, 0, 0, 0, 0, 0],
                [0, 0, 0, 0, 0, 0, 0, 0, 0, 45, 77, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
            ]
        ],
        [],
        [],
        []
    ],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 17, 0, 0, 0, 0, 0, 0, 0, 144, 0, 0, 0, 0, 0, 0, 0, 0, 5],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 222, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 9, 0, 0, 0, 0, 0, 3],
    {"warn_on_trivial_modifications": false, "log_level": 30}
]

"#;
    let expected_error =
        "address to class hash: ContractAddress(PatriciaKey(0x5000000000001005900000000000000))";
    assert_matches!(
        parse_input(input).unwrap_err(),
        DeserializationError::KeyDuplicate(key) if key ==  expected_error
    );
}
