from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.cairo_builtins import (
    BitwiseBuiltin,
    EcOpBuiltin,
    HashBuiltin,
    KeccakBuiltin,
    ModBuiltin,
    PoseidonBuiltin,
)
from starkware.starknet.core.os.proof_fact_fold import TxProofFacts, write_proof_tree_root_entry
from starkware.starknet.core.os.verify_processed_proof import verify_processed_proof

// An entry point for testing `verify_processed_proof`. It takes the output builtin without using
// it, since the simple bootloader's hints require the runner to have one.
func verify_processed_proof_test{
    output_ptr: felt*,
    pedersen_ptr: HashBuiltin*,
    range_check_ptr,
    ecdsa_ptr,
    bitwise_ptr: BitwiseBuiltin*,
    ec_op_ptr: EcOpBuiltin*,
    keccak_ptr: KeccakBuiltin*,
    poseidon_ptr: PoseidonBuiltin*,
    range_check96_ptr: felt*,
    add_mod_ptr: ModBuiltin*,
    mul_mod_ptr: ModBuiltin*,
}(flat_tree_size: felt, flat_tree: felt*, n_tx_proof_facts: felt, tx_proof_facts: TxProofFacts*) {
    alloc_locals;
    let (local root_entry: felt*) = alloc();
    write_proof_tree_root_entry(
        flat_tree_size=flat_tree_size,
        flat_tree=flat_tree,
        n_tx_proof_facts=n_tx_proof_facts,
        tx_proof_facts=tx_proof_facts,
        root_entry=root_entry,
    );
    verify_processed_proof(root_entry=root_entry);
    return ();
}
