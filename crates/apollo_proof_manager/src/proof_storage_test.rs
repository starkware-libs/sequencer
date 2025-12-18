use std::path::PathBuf;

use starknet_api::transaction::fields::Proof;
use starknet_types_core::felt::Felt;

use crate::proof_storage::{FsProofStorage, ProofStorage};

fn new_fs_proof_storage() -> FsProofStorage {
    let tmp_dir = tempfile::tempdir().unwrap();
    let persistent_root: PathBuf = tmp_dir.path().to_path_buf();
    FsProofStorage::new(persistent_root).expect("Failed to create FsProofStorage")
}

fn sample_facts_hash() -> Felt {
    Felt::from(0x1234_u64)
}

fn sample_proof() -> Proof {
    Proof::from(vec![1_u32, 2_u32, 3_u32, 4_u32, 5_u32])
}

#[test]
fn fs_proof_storage_get_before_set_returns_none() {
    let storage = new_fs_proof_storage();
    let facts_hash = sample_facts_hash();

    let res = storage.get_proof(facts_hash);
    assert!(res.is_ok());
    assert!(res.unwrap().is_none());
}

#[test]
fn fs_proof_storage_roundtrip() {
    let storage = new_fs_proof_storage();
    let facts_hash = sample_facts_hash();
    let proof = sample_proof();

    storage.set_proof(facts_hash, proof.clone()).unwrap();

    let retrieved = storage.get_proof(sample_facts_hash()).unwrap();
    assert_eq!(retrieved, Some(proof));
}
