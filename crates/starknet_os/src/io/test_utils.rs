use ark_bls12_381::Fr;
use blockifier::state::cached_state::StateMaps;
use num_bigint::BigUint;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::{Poseidon, StarkHash};

use crate::hints::hint_implementation::kzg::utils::{
    horner_eval,
    polynomial_coefficients_to_kzg_commitment,
    BLS_PRIME,
    FIELD_ELEMENTS_PER_BLOB,
};
use crate::io::os_output::OsKzgCommitmentInfo;
use crate::io::os_output_types::{
    FullCompiledClassHashUpdate,
    FullContractChanges,
    FullContractStorageUpdate,
    FullOsStateDiff,
    PartialCompiledClassHashUpdate,
    PartialContractChanges,
    PartialContractStorageUpdate,
    PartialOsStateDiff,
};

// Getters
pub(crate) trait UpdateGetter<K, V> {
    fn key(&self) -> K;
    fn new_value(&self) -> V;
}

impl UpdateGetter<StorageKey, Felt> for FullContractStorageUpdate {
    fn key(&self) -> StorageKey {
        self.key
    }

    fn new_value(&self) -> Felt {
        self.new_value
    }
}

impl UpdateGetter<StorageKey, Felt> for PartialContractStorageUpdate {
    fn key(&self) -> StorageKey {
        self.key
    }

    fn new_value(&self) -> Felt {
        self.new_value
    }
}

impl UpdateGetter<ClassHash, CompiledClassHash> for FullCompiledClassHashUpdate {
    fn key(&self) -> ClassHash {
        self.class_hash
    }

    fn new_value(&self) -> CompiledClassHash {
        self.next_compiled_class_hash
    }
}

impl UpdateGetter<ClassHash, CompiledClassHash> for PartialCompiledClassHashUpdate {
    fn key(&self) -> ClassHash {
        self.class_hash
    }

    fn new_value(&self) -> CompiledClassHash {
        self.next_compiled_class_hash
    }
}

pub(crate) trait ContractChangesGetter {
    fn addr(&self) -> ContractAddress;
    fn new_nonce(&self) -> Option<Nonce>;
    fn new_class_hash(&self) -> Option<ClassHash>;
    fn storage_changes(&self) -> &[impl UpdateGetter<StorageKey, Felt>];
}

impl ContractChangesGetter for FullContractChanges {
    fn addr(&self) -> ContractAddress {
        self.addr
    }

    fn new_nonce(&self) -> Option<Nonce> {
        Some(self.new_nonce)
    }

    fn new_class_hash(&self) -> Option<ClassHash> {
        Some(self.new_class_hash)
    }

    fn storage_changes(&self) -> &[impl UpdateGetter<StorageKey, Felt>] {
        &self.storage_changes
    }
}

impl ContractChangesGetter for PartialContractChanges {
    fn addr(&self) -> ContractAddress {
        self.addr
    }

    fn new_nonce(&self) -> Option<Nonce> {
        self.new_nonce
    }

    fn new_class_hash(&self) -> Option<ClassHash> {
        self.new_class_hash
    }

    fn storage_changes(&self) -> &[impl UpdateGetter<StorageKey, Felt>] {
        &self.storage_changes
    }
}

fn to_state_maps<CO: ContractChangesGetter, CL: UpdateGetter<ClassHash, CompiledClassHash>>(
    contracts: &[CO],
    classes: &[CL],
) -> StateMaps {
    let class_hashes = contracts
        .iter()
        .filter_map(|contract| {
            contract.new_class_hash().map(|class_hash| (contract.addr(), class_hash))
        })
        .collect();
    let nonces = contracts
        .iter()
        .filter_map(|contract| contract.new_nonce().map(|nonce| (contract.addr(), nonce)))
        .collect();
    let mut storage = std::collections::HashMap::new();
    for contract in contracts {
        for change in contract.storage_changes() {
            storage.insert((contract.addr(), change.key()), change.new_value());
        }
    }
    let compiled_class_hashes = classes
        .iter()
        .map(|class_hash_update| (class_hash_update.key(), class_hash_update.new_value()))
        .collect();
    let declared_contracts = std::collections::HashMap::new();
    StateMaps { nonces, class_hashes, storage, compiled_class_hashes, declared_contracts }
}

impl FullOsStateDiff {
    pub fn as_state_maps(&self) -> StateMaps {
        to_state_maps(&self.contracts, &self.classes)
    }
}

impl PartialOsStateDiff {
    pub fn as_state_maps(&self) -> StateMaps {
        to_state_maps(&self.contracts, &self.classes)
    }
}

/// Computes the KZG commitment for the given DA segment and verifies it matches the provided
/// commitment info.
pub fn validate_kzg_segment(da_segment: &[Felt], os_commitment_info: &OsKzgCommitmentInfo) {
    assert_eq!(os_commitment_info.n_blobs, da_segment.len().div_ceil(FIELD_ELEMENTS_PER_BLOB));

    let expected_z = Poseidon::hash(
        &Poseidon::hash_array(da_segment),
        &Poseidon::hash_array(
            &os_commitment_info
                .commitments
                .iter()
                .flat_map(|(low, high)| vec![*low, *high])
                .collect::<Vec<Felt>>(),
        ),
    );
    assert_eq!(expected_z, os_commitment_info.z);

    let felt_2_to_the_128 = Felt::TWO.pow(128u16);

    for ((chunk, commitment), (eval_low, eval_high)) in da_segment
        .chunks(FIELD_ELEMENTS_PER_BLOB)
        .zip(&os_commitment_info.commitments)
        .zip(&os_commitment_info.evals)
    {
        let computed_commitment = polynomial_coefficients_to_kzg_commitment(
            chunk.iter().map(|felt| Fr::from(felt.to_biguint())).collect(),
        )
        .unwrap();
        assert_eq!((computed_commitment.0, computed_commitment.1), *commitment);

        let computed_eval = Felt::from(horner_eval(
            &chunk.iter().map(|felt| felt.to_biguint()).collect::<Vec<BigUint>>(),
            &expected_z.to_biguint(),
            &BLS_PRIME,
        ));
        assert_eq!(computed_eval, eval_low + felt_2_to_the_128 * eval_high);
    }
}
