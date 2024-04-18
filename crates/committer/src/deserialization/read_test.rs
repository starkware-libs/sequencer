use pretty_assertions::assert_eq;
use std::collections::HashMap;

use crate::deserialization::errors::DeserializationError;
use crate::deserialization::types::{
    ContractAddress, ContractState, Input, StarknetStorageKey, StarknetStorageValue, StateDiff,
};
use crate::hash::hash_trait::HashOutput;
use crate::patricia_merkle_tree::filled_node::{ClassHash, CompiledClassHash, Nonce};
use crate::patricia_merkle_tree::types::TreeHeight;
use crate::storage::storage_trait::{StorageKey, StorageValue};
use crate::types::Felt;

use super::parse_input;

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
        ],
        [
            [
                [0, 0, 0, 0, 0, 98, 0, 0, 0, 156, 0, 0, 0, 0, 0, 11, 5, 0, 0, 0, 0, 0, 1, 0, 89, 0, 0, 0, 0, 0, 0, 0],
                [0, 0, 0, 0, 0, 0, 0, 0, 0, 45, 77, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
                [0, 0, 0, 0, 0, 98, 0, 0, 0, 156, 0, 0, 0, 0, 0, 11, 5, 0, 0, 0, 0, 0, 1, 0, 89, 0, 0, 0, 0, 0, 0, 0],
                [0, 0, 0, 0, 0, 0, 0, 0, 0, 45, 77, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]

            ],
            [
                [0, 0, 0, 1, 0, 98, 0, 0, 0, 156, 0, 0, 0, 0, 0, 11, 5, 0, 0, 0, 0, 0, 1, 0, 89, 0, 0, 0, 0, 0, 0, 0],
                [0, 0, 0, 0, 0, 0, 0, 0, 0, 45, 77, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
                [0, 0, 0, 0, 0, 98, 0, 0, 0, 156, 0, 0, 0, 0, 0, 11, 5, 0, 0, 0, 0, 0, 1, 0, 89, 0, 0, 0, 0, 0, 0, 0],
                [0, 0, 0, 0, 0, 0, 0, 0, 0, 45, 77, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
            ]

        ]
    ],
    78
]

"#;
    let expected_storage = HashMap::from([
        (
            StorageKey([14, 6, 78, 90].to_vec()),
            StorageValue([245, 90, 0, 0, 1].to_vec()),
        ),
        (
            StorageKey([14, 6, 43, 90].to_vec()),
            StorageValue([9, 0, 0, 0, 1].to_vec()),
        ),
    ]);

    let expected_address_to_class_hash = HashMap::from([
        (
            ContractAddress(Felt::from_bytes_be_slice(&[
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 1, 0, 89, 0, 0,
                0, 0, 0, 0, 0,
            ])),
            ClassHash(Felt::from_bytes_be_slice(&[
                0, 0, 0, 0, 0, 0, 0, 0, 0, 45, 77, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0,
            ])),
        ),
        (
            ContractAddress(Felt::from_bytes_be_slice(&[
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 11, 5, 0, 0, 0, 0, 0, 1, 0, 89, 0, 0,
                0, 0, 0, 0, 0,
            ])),
            ClassHash(Felt::from_bytes_be_slice(&[
                0, 0, 0, 0, 0, 0, 0, 0, 0, 45, 77, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0,
            ])),
        ),
    ]);

    let expected_address_to_nonce = HashMap::from([
        (
            ContractAddress(Felt::from_bytes_be_slice(&[
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 1, 0, 89, 0, 0,
                0, 0, 0, 0, 0,
            ])),
            Nonce(Felt::from_bytes_be_slice(&[
                0, 0, 0, 0, 0, 14, 0, 0, 0, 45, 77, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0,
            ])),
        ),
        (
            ContractAddress(Felt::from_bytes_be_slice(&[
                0, 0, 0, 0, 0, 98, 0, 0, 0, 0, 0, 0, 0, 0, 0, 11, 5, 0, 0, 0, 0, 0, 1, 0, 89, 0, 0,
                0, 0, 0, 0, 0,
            ])),
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

    let expected_current_contract_state_leaves = HashMap::from([
        (
            ContractAddress(Felt::from_bytes_be_slice(&[
                0, 0, 0, 0, 0, 98, 0, 0, 0, 156, 0, 0, 0, 0, 0, 11, 5, 0, 0, 0, 0, 0, 1, 0, 89, 0,
                0, 0, 0, 0, 0, 0,
            ])),
            ContractState {
                nonce: Nonce(Felt::from_bytes_be_slice(&[
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 45, 77, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0,
                ])),
                storage_root_hash: HashOutput(Felt::from_bytes_be_slice(&[
                    0, 0, 0, 0, 0, 98, 0, 0, 0, 156, 0, 0, 0, 0, 0, 11, 5, 0, 0, 0, 0, 0, 1, 0, 89,
                    0, 0, 0, 0, 0, 0, 0,
                ])),
                class_hash: ClassHash(Felt::from_bytes_be_slice(&[
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 45, 77, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0,
                ])),
            },
        ),
        (
            ContractAddress(Felt::from_bytes_be_slice(&[
                0, 0, 0, 1, 0, 98, 0, 0, 0, 156, 0, 0, 0, 0, 0, 11, 5, 0, 0, 0, 0, 0, 1, 0, 89, 0,
                0, 0, 0, 0, 0, 0,
            ])),
            ContractState {
                nonce: Nonce(Felt::from_bytes_be_slice(&[
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 45, 77, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0,
                ])),
                storage_root_hash: HashOutput(Felt::from_bytes_be_slice(&[
                    0, 0, 0, 0, 0, 98, 0, 0, 0, 156, 0, 0, 0, 0, 0, 11, 5, 0, 0, 0, 0, 0, 1, 0, 89,
                    0, 0, 0, 0, 0, 0, 0,
                ])),
                class_hash: ClassHash(Felt::from_bytes_be_slice(&[
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 45, 77, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0,
                ])),
            },
        ),
    ]);

    let expected_storage_updates = HashMap::from([(
        ContractAddress(Felt::from_bytes_be_slice(&[
            0, 0, 0, 0, 0, 98, 0, 0, 0, 156, 0, 0, 0, 0, 0, 11, 5, 0, 0, 0, 0, 0, 1, 0, 89, 0, 0,
            0, 0, 0, 0, 0,
        ])),
        HashMap::from([
            (
                StarknetStorageKey(Felt::from_bytes_be_slice(&[
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 1, 0, 89, 0,
                    0, 0, 0, 0, 0, 0,
                ])),
                StarknetStorageValue(Felt::from_bytes_be_slice(&[
                    0, 0, 0, 0, 0, 14, 0, 0, 0, 45, 77, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0,
                ])),
            ),
            (
                StarknetStorageKey(Felt::from_bytes_be_slice(&[
                    0, 0, 0, 0, 0, 98, 0, 0, 0, 156, 0, 0, 0, 0, 0, 11, 5, 0, 0, 0, 0, 0, 1, 0, 89,
                    0, 0, 0, 0, 0, 0, 0,
                ])),
                StarknetStorageValue(Felt::from_bytes_be_slice(&[
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 45, 77, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0,
                ])),
            ),
        ]),
    )]);

    let expected_tree_height = TreeHeight(78);
    let expected_input = Input {
        storage: expected_storage,
        state_diff: StateDiff {
            address_to_class_hash: expected_address_to_class_hash,
            address_to_nonce: expected_address_to_nonce,
            class_hash_to_compiled_class_hash: expected_class_hash_to_compiled_class_hash,
            current_contract_state_leaves: expected_current_contract_state_leaves,
            storage_updates: expected_storage_updates,
        },
        tree_height: expected_tree_height,
    };
    assert_eq!(parse_input(input.to_string()).unwrap(), expected_input);
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
        [],
        []
    ],
    78
]

"#;
    let expected_error = "storage: StorageKey([14, 6, 78, 90])";
    assert!(matches!(
        parse_input(input.to_string()).unwrap_err(),
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
        [],
        []
    ],
    78
]

"#;
    let expected_error =
    "address to class hash: ContractAddress(Felt(Felt(FieldElement { value: UnsignedInteger { limbs: [72718179, 18446744073709551615, 6917529027641073992, 16140901064500135204] } })))";
    assert!(matches!(
        parse_input(input.to_string()).unwrap_err(),
        DeserializationError::KeyDuplicate(key) if key ==  expected_error
    ));
}

#[test]
fn test_input_parsing_with_invalid_tree_size() {
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
        [],
        [],
        [],
        [],
        []
    ],
    333
]

"#;

    assert!(matches!(
        parse_input(input.to_string()).unwrap_err(),
        DeserializationError::ParsingError(_)
    ));
}
