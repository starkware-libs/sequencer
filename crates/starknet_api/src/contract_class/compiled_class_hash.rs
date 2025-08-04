use std::borrow::Cow;
use std::sync::LazyLock;

// TODO(Aviv): Remove this when starknet-types-core is updated to use the new blake
// implementation.
use blake2s::Blake2Felt252;
use cairo_lang_starknet_classes::casm_contract_class::{CasmContractClass, CasmContractEntryPoint};
use cairo_lang_starknet_classes::NestedIntList;
use itertools::Itertools;
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::{Poseidon, StarkHash};

use crate::core::CompiledClassHash;

/// Constant that defines the version of the compiled class hash algorithm.
pub static COMPILED_CLASS_V1: LazyLock<Felt> =
    LazyLock::new(|| Felt::from_bytes_be_slice(b"COMPILED_CLASS_V1"));

/// The version of the hash function used to compute the compiled class hash.
#[derive(PartialEq)]
pub enum HashVersion {
    /// Poseidon hash.
    V1,
    /// Blake2Felt252 hash.
    V2,
}

/// A trait for types that can be hashed as an entry point.
pub trait EntryPointHashable {
    fn get_selector(&self) -> Felt;
    fn get_offset(&self) -> Felt;
    fn get_builtins(&self) -> Vec<Felt>;
}

fn entry_point_hash<H: StarkHash, EH: EntryPointHashable>(entry_points: &[EH]) -> Felt {
    let mut entry_point_hash_elements = vec![];
    for entry_point in entry_points {
        entry_point_hash_elements.push(entry_point.get_selector());
        entry_point_hash_elements.push(entry_point.get_offset());
        entry_point_hash_elements.push(H::hash_array(&entry_point.get_builtins()));
    }
    H::hash_array(&entry_point_hash_elements)
}

impl EntryPointHashable for CasmContractEntryPoint {
    fn get_selector(&self) -> Felt {
        Felt::from(self.selector.clone())
    }
    fn get_offset(&self) -> Felt {
        Felt::from(self.offset)
    }
    fn get_builtins(&self) -> Vec<Felt> {
        self.builtins
            .iter()
            .map(|builtin| Felt::from_bytes_be_slice(builtin.as_bytes()))
            .collect_vec()
    }
}

/// Computes the hash of the bytecode according to the provided segment structure
/// (`bytecode_segment_lengths`). The function iterates over the bytecode, partitioning it into
/// segments as described by the `NestedIntList`. For each segment, it recursively computes a hash
/// using the provided `StarkHash` implementation. The final result is a hash representing the
/// entire bytecode structure, as required by Starknet's contract class hash computation.
fn bytecode_hash<H>(bytecode: &[Felt], bytecode_segment_lengths: &NestedIntList) -> Felt
where
    H: StarkHash,
{
    let mut bytecode_iter = bytecode.iter().copied();
    let (len, bytecode_hash) =
        bytecode_hash_node::<H>(&mut bytecode_iter, bytecode_segment_lengths);
    assert_eq!(len, bytecode.len());
    bytecode_hash
}

/// Computes the hash of a bytecode segment. See the documentation of `bytecode_hash_node` in
/// the Starknet OS.
/// Returns the length of the processed segment and its hash.
fn bytecode_hash_node<H: StarkHash>(
    iter: &mut impl Iterator<Item = Felt>,
    node: &NestedIntList,
) -> (usize, Felt) {
    match node {
        NestedIntList::Leaf(len) => {
            let data = iter.take(*len).collect_vec();
            assert_eq!(data.len(), *len);
            (*len, H::hash_array(&data))
        }
        NestedIntList::Node(nodes) => {
            // Compute `1 + poseidon(len0, hash0, len1, hash1, ...)`.
            let inner_nodes =
                nodes.iter().map(|node| bytecode_hash_node::<H>(iter, node)).collect_vec();
            let hash = H::hash_array(
                &inner_nodes.iter().flat_map(|(len, hash)| [Felt::from(*len), *hash]).collect_vec(),
            ) + Felt::ONE;
            (inner_nodes.iter().map(|(len, _)| len).sum(), hash)
        }
    }
}

/// Trait for types that can be hashed as a Starknet compiled class.
/// Used to abstract over different contract class representations.
pub trait HashableCompiledClass<EH: EntryPointHashable>: Sized {
    fn get_hashable_l1_entry_points(&self) -> &[EH];
    fn get_hashable_external_entry_points(&self) -> &[EH];
    fn get_hashable_constructor_entry_points(&self) -> &[EH];
    fn get_bytecode(&self) -> Vec<Felt>;
    fn get_bytecode_segment_lengths(&self) -> Cow<'_, NestedIntList>;

    /// Returns the compiled class hash using the specified hash version.
    fn hash(&self, hash_version: &HashVersion) -> CompiledClassHash {
        match hash_version {
            HashVersion::V1 => hash_inner::<Poseidon, EH>(self),
            HashVersion::V2 => hash_inner::<Blake2Felt252, EH>(self),
        }
    }
}

/// Computes the compiled class hash for a given hashable class using the specified hash algorithm.
fn hash_inner<H: StarkHash, EH: EntryPointHashable>(
    hashable_class: &impl HashableCompiledClass<EH>,
) -> CompiledClassHash {
    let external_funcs_hash =
        entry_point_hash::<H, EH>(hashable_class.get_hashable_external_entry_points());
    let l1_handlers_hash = entry_point_hash::<H, EH>(hashable_class.get_hashable_l1_entry_points());
    let constructors_hash =
        entry_point_hash::<H, EH>(hashable_class.get_hashable_constructor_entry_points());

    let bytecode_hash = bytecode_hash::<H>(
        &hashable_class.get_bytecode(),
        &hashable_class.get_bytecode_segment_lengths(),
    );

    // Compute total hash by hashing each component on top of the previous one.
    CompiledClassHash(H::hash_array(&[
        *COMPILED_CLASS_V1,
        external_funcs_hash,
        l1_handlers_hash,
        constructors_hash,
        bytecode_hash,
    ]))
}

impl HashableCompiledClass<CasmContractEntryPoint> for CasmContractClass {
    fn get_hashable_l1_entry_points(&self) -> &[CasmContractEntryPoint] {
        &self.entry_points_by_type.l1_handler
    }

    fn get_hashable_external_entry_points(&self) -> &[CasmContractEntryPoint] {
        &self.entry_points_by_type.external
    }

    fn get_hashable_constructor_entry_points(&self) -> &[CasmContractEntryPoint] {
        &self.entry_points_by_type.constructor
    }

    fn get_bytecode(&self) -> Vec<Felt> {
        self.bytecode.iter().map(|big_uint| Felt::from(&big_uint.value)).collect()
    }

    /// Returns the lengths of the bytecode segments.
    /// If the length field is missing, the entire bytecode is considered a single segment.
    fn get_bytecode_segment_lengths(&self) -> Cow<'_, NestedIntList> {
        match &self.bytecode_segment_lengths {
            Some(bytecode_segment_lengths) => Cow::Borrowed(bytecode_segment_lengths),
            None => Cow::Owned(NestedIntList::Leaf(self.bytecode.len())),
        }
    }
}
