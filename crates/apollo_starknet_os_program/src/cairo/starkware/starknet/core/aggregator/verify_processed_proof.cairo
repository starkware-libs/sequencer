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

// The Blake program hash of the circuit verifier the aggregator runs, as the simple bootloader
// computes it.
// TODO(Einat): the hash of the canonical_small test verifier; replace it with the production
// verifier's.
const CIRCUIT_VERIFIER_PROGRAM_HASH = (
    0x764dc214c7f45a6899b05d42ba4d23d5849e0bf0383ec951f000b1742107fdd
);

// The output of a simple bootloader task: its size and program hash and verifier output digest.
const VERIFIER_TASK_OUTPUT_SIZE = 2 + BLAKE2S_DIGEST_N_WORDS;

// Runs the circuit verifier on a processed proof as a simple bootloader task.
func verify_processed_proof{
    pedersen_ptr: HashBuiltin*,
    range_check_ptr,
    ec_op_ptr: EcOpBuiltin*,
    poseidon_ptr: PoseidonBuiltin*,
}(processed_proof_output_low: felt, processed_proof_output_high: felt) {
    alloc_locals;
    // Write the output the simple bootloader must produce: the number of tasks, followed by the
    // verifier task's size, program hash and output, which is the verification digest.
    let (local verifier_bootloader_output: felt*) = alloc();
    assert verifier_bootloader_output[0] = 1;
    assert verifier_bootloader_output[1] = VERIFIER_TASK_OUTPUT_SIZE;
    assert verifier_bootloader_output[2] = CIRCUIT_VERIFIER_PROGRAM_HASH;
    compute_verification_digest(
        processed_proof_output_low=processed_proof_output_low,
        processed_proof_output_high=processed_proof_output_high,
        verification_digest=&verifier_bootloader_output[3],
    );

    // Create builtin pointer placeholders the bootloader expects.
    let output_ptr = verifier_bootloader_output;
    let ecdsa_ptr = cast(0, SignatureBuiltin*);
    let bitwise_ptr = cast(0, BitwiseBuiltin*);
    let keccak_ptr = cast(0, KeccakBuiltin*);
    let range_check96_ptr = 0;
    let add_mod_ptr = cast(0, ModBuiltin*);
    let mul_mod_ptr = cast(0, ModBuiltin*);
    // The bootloader writes its output over the expected output, so as Cairo memory is write-once,
    // the run fails unless the two are equal.
    with_attr error_message("The processed proof does not verify against its output digest.") {
        with output_ptr, ecdsa_ptr, bitwise_ptr, keccak_ptr, range_check96_ptr, add_mod_ptr, mul_mod_ptr {
            run_simple_bootloader();
        }
    }
    return ();
}
