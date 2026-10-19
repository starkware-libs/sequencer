from starkware.cairo.bootloaders.simple_bootloader.run_simple_bootloader import (
    run_simple_bootloader,
)
from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.cairo_blake2s.blake2s import blake_with_opcode
from starkware.cairo.common.cairo_builtins import (
    BitwiseBuiltin,
    EcOpBuiltin,
    HashBuiltin,
    KeccakBuiltin,
    ModBuiltin,
    PoseidonBuiltin,
    SignatureBuiltin,
)
from starkware.starknet.core.os.output import OsCarriedOutputs, ProofFacts
from starkware.starknet.core.os.proof_fact_fold import (
    BLAKE2S_DIGEST_N_WORDS,
    PROOF_ENTRY_N_WORDS,
    write_proof_tree_root_entry,
)

// The Blake program hash of the circuit verifier, as the simple bootloader computes it.
// TODO(Einat): the hash of the canonical_small test verifier; replace it with the production
// verifier's.
const CIRCUIT_VERIFIER_PROGRAM_HASH = (
    0x764dc214c7f45a6899b05d42ba4d23d5849e0bf0383ec951f000b1742107fdd
);

// The output of a simple bootloader task: its size and program hash and verifier output digest.
const VERIFIER_TASK_OUTPUT_SIZE = 2 + BLAKE2S_DIGEST_N_WORDS;

// Verifies the proofs of the transactions' proof facts between `initial_carried_outputs` and
// `final_carried_outputs`, if there are any: computes the root of their proof tree, whose shape the
// proof's prover chose, and runs the circuit verifier on the root's proof.
func verify_tx_proof_facts{range_check_ptr}(
    initial_carried_outputs: OsCarriedOutputs*, final_carried_outputs: OsCarriedOutputs*
) {
    alloc_locals;
    let n_proof_facts = (
        final_carried_outputs.tx_proof_facts - initial_carried_outputs.tx_proof_facts
    ) / ProofFacts.SIZE;
    if (n_proof_facts == 0) {
        return ();
    }

    local flat_tree_size: felt;
    local flat_tree: felt*;
    %{ GuessProofFactsTree %}
    let (root_entry: felt*) = alloc();
    write_proof_tree_root_entry(
        flat_tree_size=flat_tree_size,
        flat_tree=flat_tree,
        n_proof_facts=n_proof_facts,
        proof_facts_list=initial_carried_outputs.tx_proof_facts,
        root_entry=root_entry,
    );
    %{ LoadCircuitVerifierTask %}
    verify_processed_proof(root_entry=root_entry);
    return ();
}

// Runs the circuit verifier, as a simple bootloader task, on the processed proof: the proof of the
// root of a proof tree, whose entry (circuit hash and output digest) is `root_entry`.
func verify_processed_proof{range_check_ptr}(root_entry: felt*) {
    alloc_locals;
    // Write the output the simple bootloader must produce: the number of tasks, followed by the
    // verifier task's size, program hash and output, which is the blake2s hash of the root entry.
    let (local verifier_bootloader_output: felt*) = alloc();
    assert verifier_bootloader_output[0] = 1;
    assert verifier_bootloader_output[1] = VERIFIER_TASK_OUTPUT_SIZE;
    assert verifier_bootloader_output[2] = CIRCUIT_VERIFIER_PROGRAM_HASH;
    blake_with_opcode(len=PROOF_ENTRY_N_WORDS, data=root_entry, out=&verifier_bootloader_output[3]);

    // The circuit verifier uses only the output and range check builtins, and the bootloader
    // hashes it with blake2s, so the other builtins' pointers are never used: the bootloader passes
    // a task the pointers of the builtins it uses only.
    let output_ptr = verifier_bootloader_output;
    let pedersen_ptr = cast(0, HashBuiltin*);
    let ecdsa_ptr = cast(0, SignatureBuiltin*);
    let bitwise_ptr = cast(0, BitwiseBuiltin*);
    let ec_op_ptr = cast(0, EcOpBuiltin*);
    let keccak_ptr = cast(0, KeccakBuiltin*);
    let poseidon_ptr = cast(0, PoseidonBuiltin*);
    let range_check96_ptr = 0;
    let add_mod_ptr = cast(0, ModBuiltin*);
    let mul_mod_ptr = cast(0, ModBuiltin*);
    with_attr error_message("The processed proof does not verify against its proof facts.") {
        run_simple_bootloader{
            output_ptr=output_ptr,
            pedersen_ptr=pedersen_ptr,
            ecdsa_ptr=ecdsa_ptr,
            bitwise_ptr=bitwise_ptr,
            ec_op_ptr=ec_op_ptr,
            keccak_ptr=keccak_ptr,
            poseidon_ptr=poseidon_ptr,
            range_check96_ptr=range_check96_ptr,
            add_mod_ptr=add_mod_ptr,
            mul_mod_ptr=mul_mod_ptr,
        }();
    }
    // Sanity check that output_ptr has advanced by exactly the expected bootloader output: the
    // number of tasks and the verifier task's output.
    assert output_ptr = verifier_bootloader_output + 1 + VERIFIER_TASK_OUTPUT_SIZE;
    return ();
}
