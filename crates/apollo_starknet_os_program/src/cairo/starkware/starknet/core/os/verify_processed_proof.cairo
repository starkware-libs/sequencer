from starkware.cairo.bootloaders.simple_bootloader.run_simple_bootloader import (
    run_simple_bootloader,
)
from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.cairo_builtins import (
    BitwiseBuiltin,
    EcOpBuiltin,
    HashBuiltin,
    KeccakBuiltin,
    ModBuiltin,
    PoseidonBuiltin,
    SignatureBuiltin,
)
from starkware.starknet.core.os.proof_fact_fold import (
    BLAKE2S_DIGEST_N_WORDS,
    compute_verification_digest,
)

// The Blake program hash of the circuit verifier, as the simple bootloader computes it.
// TODO(Einat): the hash of the canonical_small test verifier; replace it with the production
// verifier's.
const CIRCUIT_VERIFIER_PROGRAM_HASH = (
    0x764dc214c7f45a6899b05d42ba4d23d5849e0bf0383ec951f000b1742107fdd
);

// The output of a simple bootloader task: its size and program hash and verifier output digest.
const VERIFIER_TASK_OUTPUT_SIZE = 2 + BLAKE2S_DIGEST_N_WORDS;

// Runs the circuit verifier, as a simple bootloader task, on the processed proof of a
// transaction's proof facts, which fill both of the multiverifier's verifier slots.
// TODO(Einat): modify the function to take a tree of proof facts instead of just one.
func verify_processed_proof{
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
}(proof_facts_size: felt, proof_facts: felt*) {
    alloc_locals;
    // Write the output the simple bootloader must produce: the number of tasks, followed by the
    // verifier task's size, program hash and output, which is the verification digest.
    let (local verifier_bootloader_output: felt*) = alloc();
    assert verifier_bootloader_output[0] = 1;
    assert verifier_bootloader_output[1] = VERIFIER_TASK_OUTPUT_SIZE;
    assert verifier_bootloader_output[2] = CIRCUIT_VERIFIER_PROGRAM_HASH;
    compute_verification_digest(
        left_proof_facts_size=proof_facts_size,
        left_proof_facts=proof_facts,
        right_proof_facts_size=proof_facts_size,
        right_proof_facts=proof_facts,
        verification_digest=&verifier_bootloader_output[3],
    );

    // The bootloader types the signature and range_check96 builtin pointers differently than the
    // OS.
    let output_ptr = verifier_bootloader_output;
    let signature_ptr = cast(ecdsa_ptr, SignatureBuiltin*);
    let range_check96 = cast(range_check96_ptr, felt);
    with_attr error_message("The processed proof does not verify against its proof facts.") {
        run_simple_bootloader{
            output_ptr=output_ptr, ecdsa_ptr=signature_ptr, range_check96_ptr=range_check96
        }();
    }
    let ecdsa_ptr = cast(signature_ptr, felt);
    let range_check96_ptr = cast(range_check96, felt*);
    return ();
}
