from starkware.cairo.common.alloc import alloc
from starkware.starknet.core.os.output import ProofFacts
from starkware.starknet.core.os.proof_fact_fold import write_proof_tree_root_entry
from starkware.starknet.core.os.verify_processed_proof import verify_processed_proof

// An entry point for testing `verify_processed_proof`. It takes the output builtin without using
// it, since the simple bootloader's hints require the runner to have one.
func verify_processed_proof_test{output_ptr: felt*, range_check_ptr}(
    n_proof_facts: felt, proof_facts_list: ProofFacts*
) {
    alloc_locals;
    let (local root_entry: felt*) = alloc();
    write_proof_tree_root_entry(
        n_proof_facts=n_proof_facts, proof_facts_list=proof_facts_list, root_entry=root_entry
    );
    verify_processed_proof(root_entry=root_entry);
    return ();
}
