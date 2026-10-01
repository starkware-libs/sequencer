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
from starkware.cairo.common.math import split_int
from starkware.cairo.common.memcpy import memcpy
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
    let (local verifier_bootloader_output: felt*) = alloc();

    // Create builtin pointer placeholders the bootloader expects.
    let output_ptr = verifier_bootloader_output;
    let ecdsa_ptr = cast(0, SignatureBuiltin*);
    let bitwise_ptr = cast(0, BitwiseBuiltin*);
    let keccak_ptr = cast(0, KeccakBuiltin*);
    let range_check96_ptr = 0;
    let add_mod_ptr = cast(0, ModBuiltin*);
    let mul_mod_ptr = cast(0, ModBuiltin*);
    with output_ptr, ecdsa_ptr, bitwise_ptr, keccak_ptr, range_check96_ptr, add_mod_ptr, mul_mod_ptr {
        run_simple_bootloader();
    }

    // Assert the number of tasks, the task's size, and the verifier program hash.
    assert verifier_bootloader_output[0] = 1;
    assert verifier_bootloader_output[1] = VERIFIER_TASK_OUTPUT_SIZE;
    assert verifier_bootloader_output[2] = CIRCUIT_VERIFIER_PROGRAM_HASH;

    let (local processed_proof_output_digest: felt*) = alloc();
    split_int(
        value=processed_proof_output_low,
        n=4,
        base=2 ** 32,
        bound=2 ** 32,
        output=processed_proof_output_digest,
    );
    split_int(
        value=processed_proof_output_high,
        n=4,
        base=2 ** 32,
        bound=2 ** 32,
        output=&processed_proof_output_digest[4],
    );
    let (verification_digest) = compute_verification_digest(
        processed_proof_output_digest=processed_proof_output_digest
    );
    with_attr error_message("The circuit verifier's output does not match the processed proof.") {
        // The bootloader already wrote its output, so the copy asserts the words are equal.
        memcpy(
            dst=&verifier_bootloader_output[3], src=verification_digest, len=BLAKE2S_DIGEST_N_WORDS
        );
    }
    return ();
}
