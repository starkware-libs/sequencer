use indexmap::indexmap;

use crate::block_hash::state_diff_hash::{
    calculate_state_diff_hash, chain_declared_classes, chain_deprecated_declared_classes,
    chain_nonces, chain_storage_diffs, chain_updated_contracts,
};
use crate::block_hash::test_utils::get_state_diff;
use crate::core::{ClassHash, CompiledClassHash, Nonce, StateDiffCommitment};
use crate::crypto::utils::HashChain;
use crate::felt;
use crate::hash::PoseidonHash;

#[test]
fn test_state_diff_hash_regression() {
    let state_diff = get_state_diff();

    let expected_hash = StateDiffCommitment(PoseidonHash(felt!(
        "0x0281f5966e49ad7dad9323826d53d1d27c0c4e6ebe5525e2e2fbca549bfa0a67"
    )));

    assert_eq!(expected_hash, calculate_state_diff_hash(&state_diff));
}

#[test]
fn test_sorting_deployed_contracts() {
    let deployed_contracts_0 = indexmap! {
        0u64.into() => ClassHash(3u64.into()),
        1u64.into() => ClassHash(2u64.into()),
    };
    let replaced_classes_0 = indexmap! {
        2u64.into() => ClassHash(1u64.into()),
    };
    let deployed_contracts_1 = indexmap! {
        2u64.into() => ClassHash(1u64.into()),
        0u64.into() => ClassHash(3u64.into()),
    };
    let replaced_classes_1 = indexmap! {
        1u64.into() => ClassHash(2u64.into()),
    };
    assert_eq!(
        chain_updated_contracts(&deployed_contracts_0, &replaced_classes_0, HashChain::new())
            .get_poseidon_hash(),
        chain_updated_contracts(&deployed_contracts_1, &replaced_classes_1, HashChain::new())
            .get_poseidon_hash(),
    );
}

#[test]
fn test_sorting_declared_classes() {
    let declared_classes_0 = indexmap! {
        ClassHash(0u64.into()) => CompiledClassHash(3u64.into()),
        ClassHash(1u64.into()) => CompiledClassHash(2u64.into()),
    };
    let declared_classes_1 = indexmap! {
        ClassHash(1u64.into()) => CompiledClassHash(2u64.into()),
        ClassHash(0u64.into()) => CompiledClassHash(3u64.into()),
    };
    assert_eq!(
        chain_declared_classes(&declared_classes_0, HashChain::new()).get_poseidon_hash(),
        chain_declared_classes(&declared_classes_1, HashChain::new()).get_poseidon_hash(),
    );
}

#[test]
fn test_sorting_deprecated_declared_classes() {
    let deprecated_declared_classes_0 = vec![ClassHash(0u64.into()), ClassHash(1u64.into())];
    let deprecated_declared_classes_1 = vec![ClassHash(1u64.into()), ClassHash(0u64.into())];
    assert_eq!(
        chain_deprecated_declared_classes(&deprecated_declared_classes_0, HashChain::new())
            .get_poseidon_hash(),
        chain_deprecated_declared_classes(&deprecated_declared_classes_1, HashChain::new())
            .get_poseidon_hash(),
    );
}

#[test]
fn test_sorting_storage_diffs() {
    let storage_diffs_0 = indexmap! {
        0u64.into() => indexmap! {
            1u64.into() => 2u64.into(),
            3u64.into() => 4u64.into(),
        },
        5u64.into() => indexmap! {
            6u64.into() => 7u64.into(),
        },
    };
    let storage_diffs_1 = indexmap! {
        5u64.into() => indexmap! {
            6u64.into() => 7u64.into(),
        },
        0u64.into() => indexmap! {
            3u64.into() => 4u64.into(),
            1u64.into() => 2u64.into(),
        },
    };
    assert_eq!(
        chain_storage_diffs(&storage_diffs_0, HashChain::new()).get_poseidon_hash(),
        chain_storage_diffs(&storage_diffs_1, HashChain::new()).get_poseidon_hash(),
    );
}

#[test]
fn test_empty_storage_diffs() {
    let storage_diffs_0 = indexmap! {
        0u64.into() => indexmap! {
            1u64.into() => 2u64.into(),
        },
        3u64.into() => indexmap! {
        },
    };
    let storage_diffs_1 = indexmap! {
        0u64.into() => indexmap! {
            1u64.into() => 2u64.into(),
        },
    };
    assert_eq!(
        chain_storage_diffs(&storage_diffs_0, HashChain::new()).get_poseidon_hash(),
        chain_storage_diffs(&storage_diffs_1, HashChain::new()).get_poseidon_hash(),
    );
}

#[test]
fn test_sorting_nonces() {
    let nonces_0 = indexmap! {
        0u64.into() => Nonce(3u64.into()),
        1u64.into() => Nonce(2u64.into()),
    };
    let nonces_1 = indexmap! {
        1u64.into() => Nonce(2u64.into()),
        0u64.into() => Nonce(3u64.into()),
    };
    assert_eq!(
        chain_nonces(&nonces_0, HashChain::new()).get_poseidon_hash(),
        chain_nonces(&nonces_1, HashChain::new()).get_poseidon_hash(),
    );
}
