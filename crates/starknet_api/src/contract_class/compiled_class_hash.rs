use std::sync::LazyLock;

use cairo_lang_starknet_classes::casm_contract_class::CasmContractEntryPoint;
use cairo_lang_starknet_classes::NestedIntList;
use itertools::Itertools;
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::StarkHash;

/// Constant that defines the version of the compiled class hash algorithm.
pub static COMPILED_CLASS_V1: LazyLock<Felt> =
    LazyLock::new(|| Felt::from_bytes_be_slice(b"COMPILED_CLASS_V1"));

/// A trait for types that can be hashed as an entry point.
pub trait EntryPointHashable {
    fn get_selector(&self) -> Felt;
    fn get_offset(&self) -> Felt;
    fn get_builtins(&self) -> Vec<Felt>;
}

#[allow(dead_code)]
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
#[allow(dead_code)]
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
#[allow(dead_code)]
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
