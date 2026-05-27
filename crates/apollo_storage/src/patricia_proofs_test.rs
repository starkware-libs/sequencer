use std::collections::HashMap;

use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::hash::HashOutput;
use starknet_committer::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use starknet_committer::patricia_merkle_tree::types::ContractsTrieProof;
use starknet_types_core::felt::Felt;

use crate::patricia_proofs::{
    PatriciaProofsStorageReader,
    PatriciaProofsStorageWriter,
    StarknetForestProofs,
};
use crate::test_utils::get_test_storage;

fn empty_proofs() -> StarknetForestProofs {
    StarknetForestProofs {
        classes_trie_proof: HashMap::new(),
        contracts_trie_proof: ContractsTrieProof { nodes: HashMap::new(), leaves: HashMap::new() },
        contracts_trie_storage_proofs: HashMap::new(),
    }
}

fn proofs_with_leaf() -> StarknetForestProofs {
    let address = ContractAddress::try_from(Felt::from(1_u128)).unwrap();
    let contract_state = ContractState {
        nonce: Nonce(Felt::from(7_u128)),
        storage_root_hash: HashOutput(Felt::from(8_u128)),
        class_hash: ClassHash(Felt::from(9_u128)),
    };
    let mut proofs = empty_proofs();
    proofs.contracts_trie_proof.leaves.insert(address, contract_state);
    proofs
}

#[test]
fn append_and_get_patricia_proofs() {
    let (reader, mut writer) = get_test_storage().0;
    let height = BlockNumber(5);
    let proofs = proofs_with_leaf();

    // No proofs stored for the height yet.
    assert_eq!(reader.begin_ro_txn().unwrap().get_patricia_proofs(height).unwrap(), None);

    writer
        .begin_rw_txn()
        .unwrap()
        .append_patricia_proofs(height, &proofs)
        .unwrap()
        .commit()
        .unwrap();

    assert_eq!(reader.begin_ro_txn().unwrap().get_patricia_proofs(height).unwrap(), Some(proofs));
    // A different height is still empty.
    assert_eq!(reader.begin_ro_txn().unwrap().get_patricia_proofs(BlockNumber(6)).unwrap(), None);
}

#[test]
fn append_and_get_empty_patricia_proofs() {
    let (reader, mut writer) = get_test_storage().0;
    let height = BlockNumber(0);
    let proofs = empty_proofs();

    writer
        .begin_rw_txn()
        .unwrap()
        .append_patricia_proofs(height, &proofs)
        .unwrap()
        .commit()
        .unwrap();

    assert_eq!(reader.begin_ro_txn().unwrap().get_patricia_proofs(height).unwrap(), Some(proofs));
}

#[test]
fn revert_patricia_proofs() {
    let (reader, mut writer) = get_test_storage().0;
    let height = BlockNumber(5);

    writer
        .begin_rw_txn()
        .unwrap()
        .append_patricia_proofs(height, &proofs_with_leaf())
        .unwrap()
        .revert_patricia_proofs(height)
        .unwrap()
        .commit()
        .unwrap();

    assert_eq!(reader.begin_ro_txn().unwrap().get_patricia_proofs(height).unwrap(), None);

    // Reverting a height with no stored proofs is a no-op.
    writer
        .begin_rw_txn()
        .unwrap()
        .revert_patricia_proofs(BlockNumber(99))
        .unwrap()
        .commit()
        .unwrap();
}
