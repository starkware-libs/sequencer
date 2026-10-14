from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.cairo_blake2s.blake2s import blake_with_opcode, encode_felt252s_to_u32s
from starkware.cairo.common.memcpy import memcpy
from starkware.starknet.core.os.output import BLAKE2S_DIGEST_N_WORDS, TxProofFacts
from starkware.starknet.core.os.virtual_os_output import ProofHeader

const PROOF_ENTRY_N_WORDS = 2 * BLAKE2S_DIGEST_N_WORDS;

// A proof tree is passed as a list with one entry per node: FLAT_TREE_FOLD for a node that
// verifies two child proofs, and FLAT_TREE_LEAF for a transaction's proof. Each fold is followed
// by the list of its left subtree, then the list of its right subtree. For example, [1, 1, 0, 0, 0]
// is a fold whose left child folds two leaves and whose right child is a leaf.
const FLAT_TREE_LEAF = 0;
const FLAT_TREE_FOLD = 1;

// Writes a transaction's leaf output digest (blake2s hash of its proof facts from program hash
// onwards) to `output_digest`.
func compute_leaf_output_digest{range_check_ptr}(
    proof_facts_size: felt, proof_facts: felt*, output_digest: felt*
) {
    alloc_locals;
    let (local encoded_words: felt*) = alloc();
    let encoded_words_len = encode_felt252s_to_u32s(
        packed_values_len=proof_facts_size - ProofHeader.program_hash,
        packed_values=&proof_facts[ProofHeader.program_hash],
        unpacked_u32s=encoded_words,
    );
    blake_with_opcode(len=encoded_words_len, data=encoded_words, out=output_digest);
    return ();
}

// Writes to `root_entry` the entry (circuit hash and output digest) of the root of a proof tree,
// whose leaves are the given transactions' proof facts, in order.
//
// `flat_tree` is the tree's list of nodes (see FLAT_TREE_LEAF). For a tree of a single
// transaction's proof facts, the root multiverifier verifies that leaf in both of its slots.
func write_proof_tree_root_entry{range_check_ptr}(
    flat_tree_size: felt,
    flat_tree: felt*,
    n_tx_proof_facts: felt,
    tx_proof_facts: TxProofFacts*,
    root_entry: felt*,
) {
    alloc_locals;
    if (flat_tree[0] == FLAT_TREE_LEAF) {
        assert flat_tree_size = 1;
        assert n_tx_proof_facts = 1;
        write_multiverifier_circuit_hash(circuit_hash=root_entry);
        let (local leaf_entries: felt*) = alloc();
        write_leaf_proof_entry(
            proof_facts_size=tx_proof_facts.proof_facts_size,
            proof_facts=tx_proof_facts.proof_facts,
            proof_entry=leaf_entries,
        );
        memcpy(dst=&leaf_entries[PROOF_ENTRY_N_WORDS], src=leaf_entries, len=PROOF_ENTRY_N_WORDS);
        blake_with_opcode(
            len=2 * PROOF_ENTRY_N_WORDS, data=leaf_entries, out=&root_entry[BLAKE2S_DIGEST_N_WORDS]
        );
        return ();
    }

    let next_tx_proof_facts = tx_proof_facts;
    with next_tx_proof_facts {
        let flat_tree_end = write_subtree_entry(flat_tree=flat_tree, entry=root_entry);
    }
    // Every node and every transaction's proof facts must be used.
    assert flat_tree_end = &flat_tree[flat_tree_size];
    assert next_tx_proof_facts = &tx_proof_facts[n_tx_proof_facts];
    return ();
}

// Writes to `entry` the entry of the subtree that starts at `flat_tree`, and returns the end of
// the subtree in `flat_tree`. Each leaf uses the next transaction's proof facts.
func write_subtree_entry{range_check_ptr, next_tx_proof_facts: TxProofFacts*}(
    flat_tree: felt*, entry: felt*
) -> felt* {
    alloc_locals;
    if (flat_tree[0] == FLAT_TREE_LEAF) {
        write_leaf_proof_entry(
            proof_facts_size=next_tx_proof_facts.proof_facts_size,
            proof_facts=next_tx_proof_facts.proof_facts,
            proof_entry=entry,
        );
        let next_tx_proof_facts = &next_tx_proof_facts[1];
        return &flat_tree[1];
    }

    assert flat_tree[0] = FLAT_TREE_FOLD;
    write_multiverifier_circuit_hash(circuit_hash=entry);
    // The children's entries, written where the fold's output digest hashes them.
    let (local children_entries: felt*) = alloc();
    let right_subtree = write_subtree_entry(flat_tree=&flat_tree[1], entry=children_entries);
    let subtree_end = write_subtree_entry(
        flat_tree=right_subtree, entry=&children_entries[PROOF_ENTRY_N_WORDS]
    );
    blake_with_opcode(
        len=2 * PROOF_ENTRY_N_WORDS, data=children_entries, out=&entry[BLAKE2S_DIGEST_N_WORDS]
    );
    return subtree_end;
}

// Writes a transaction's entry in the multiverifier output to `proof_entry`.
func write_leaf_proof_entry{range_check_ptr}(
    proof_facts_size: felt, proof_facts: felt*, proof_entry: felt*
) {
    write_leaf_verifier_circuit_hash(circuit_hash=proof_entry);
    compute_leaf_output_digest(
        proof_facts_size=proof_facts_size,
        proof_facts=proof_facts,
        output_digest=&proof_entry[BLAKE2S_DIGEST_N_WORDS],
    );
    return ();
}

// Writes the leaf verifier's circuit hash to `circuit_hash` as eight little-endian 32-bit words.
// TODO(Einat): accept the leaf verifiers of all the registry's trace sizes.
func write_leaf_verifier_circuit_hash(circuit_hash: felt*) {
    assert circuit_hash[0] = 0xd2d85a42;
    assert circuit_hash[1] = 0x79697b22;
    assert circuit_hash[2] = 0x3a41a061;
    assert circuit_hash[3] = 0x011cb393;
    assert circuit_hash[4] = 0x7a040ec9;
    assert circuit_hash[5] = 0x4508f4ca;
    assert circuit_hash[6] = 0x42239409;
    assert circuit_hash[7] = 0x60f3baea;
    return ();
}

// Writes the multiverifier's circuit hash to `circuit_hash` as eight little-endian 32-bit words.
func write_multiverifier_circuit_hash(circuit_hash: felt*) {
    assert circuit_hash[0] = 0xa5989715;
    assert circuit_hash[1] = 0x2377c07a;
    assert circuit_hash[2] = 0xc6d1e844;
    assert circuit_hash[3] = 0x54f0a04d;
    assert circuit_hash[4] = 0x8be65a7d;
    assert circuit_hash[5] = 0xfd73c261;
    assert circuit_hash[6] = 0x9078e728;
    assert circuit_hash[7] = 0x973f680f;
    return ();
}
