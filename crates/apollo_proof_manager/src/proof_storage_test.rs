use std::path::PathBuf;

use starknet_api::transaction::fields::{Proof, ProofFacts};
use starknet_api::transaction::TransactionHash;
use starknet_types_core::felt::Felt;

use crate::proof_storage::{FsProofStorage, ProofStorage};

fn new_fs_proof_storage() -> FsProofStorage {
    let tmp_dir = tempfile::tempdir().unwrap();
    let persistent_root: PathBuf = tmp_dir.path().to_path_buf();
    FsProofStorage::new(persistent_root).expect("Failed to create FsProofStorage")
}

fn sample_proof_facts() -> ProofFacts {
    ProofFacts::from(vec![Felt::from(0x1234_u64), Felt::from(0x5678_u64)])
}

fn sample_facts_hash() -> Felt {
    sample_proof_facts().hash()
}

fn sample_proof() -> Proof {
    Proof::from(vec![1_u8, 2_u8, 3_u8, 4_u8, 5_u8])
}

fn sample_tx_hash() -> TransactionHash {
    TransactionHash(starknet_types_core::felt::Felt::from(0xabcd_u64))
}

#[tokio::test]
async fn fs_proof_storage_get_before_set_returns_none() {
    let storage = new_fs_proof_storage();
    let facts_hash = sample_facts_hash();
    let tx_hash = sample_tx_hash();

    let res = storage.get_proof(facts_hash, tx_hash).await;
    assert!(res.is_ok());
    assert!(res.unwrap().is_none());
}

#[tokio::test]
async fn fs_proof_storage_roundtrip() {
    let storage = new_fs_proof_storage();
    let proof = sample_proof();
    let facts_hash = sample_facts_hash();
    let tx_hash = sample_tx_hash();

    storage.set_proof(facts_hash, tx_hash, proof.clone()).await.unwrap();

    let retrieved = storage.get_proof(facts_hash, tx_hash).await.unwrap();
    assert_eq!(retrieved, Some(proof));
}
