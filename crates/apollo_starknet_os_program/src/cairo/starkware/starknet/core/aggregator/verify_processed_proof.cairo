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
from starkware.starknet.core.os.output import OsOutputHeader
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

// The output of a simple bootloader task: its size and program hash, followed by the program's
// output, which for the circuit verifier is its verification digest.
const VERIFIER_TASK_OUTPUT_SIZE = 2 + BLAKE2S_DIGEST_N_WORDS;

// Verifies the processed proof of the aggregated blocks' transaction with proof facts, if they have
// one, against the output digest in their combined header.
func verify_aggregated_processed_proof{
    pedersen_ptr: HashBuiltin*,
    range_check_ptr,
    ec_op_ptr: EcOpBuiltin*,
    poseidon_ptr: PoseidonBuiltin*,
}(header: OsOutputHeader*) {
    if (header.n_proof_facts_transactions == 0) {
        return ();
    }
    %{ EnterCircuitVerifierTaskScope %}
    verify_processed_proof(
        processed_proof_output_low=header.processed_proof_output_low,
        processed_proof_output_high=header.processed_proof_output_high,
    );
    %{ vm_exit_scope() %}
    return ();
}

// Runs the circuit verifier on a processed proof as a simple bootloader task, and checks that the
// verifier is the pinned one and that it outputs the verification digest of the processed proof
// output digest, packed into `processed_proof_output_low` and `processed_proof_output_high` by
// `pack_output_digest`.
//
// Hint arguments:
// simple_bootloader_input - the circuit verifier task, in the current scope.
func verify_processed_proof{
    pedersen_ptr: HashBuiltin*,
    range_check_ptr,
    ec_op_ptr: EcOpBuiltin*,
    poseidon_ptr: PoseidonBuiltin*,
}(processed_proof_output_low: felt, processed_proof_output_high: felt) {
    alloc_locals;
    // The bootloader writes the verifier's output to a scratch segment rather than to the
    // aggregator's output, which has a fixed layout.
    let (local verifier_bootloader_output: felt*) = alloc();
    // The aggregator has none of the builtins below. The verifier's program hash is pinned and
    // its program uses only the output and range check builtins, so these pointers are never
    // passed to it, and the bootloader validates that they did not move.
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

    // The number of tasks is written by a hint, and the task's size bounds the cells of its
    // output, so check both before reading the verification digest.
    assert verifier_bootloader_output[0] = 1;
    assert verifier_bootloader_output[1] = VERIFIER_TASK_OUTPUT_SIZE;
    assert verifier_bootloader_output[2] = CIRCUIT_VERIFIER_PROGRAM_HASH;

    // Unpacks each half into its four little-endian 32-bit words; fails if a half is not below
    // 2^128.
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
        assert_digests_equal(
            digest=&verifier_bootloader_output[3], expected_digest=verification_digest
        );
    }
    return ();
}

func assert_digests_equal(digest: felt*, expected_digest: felt*) {
    assert digest[0] = expected_digest[0];
    assert digest[1] = expected_digest[1];
    assert digest[2] = expected_digest[2];
    assert digest[3] = expected_digest[3];
    assert digest[4] = expected_digest[4];
    assert digest[5] = expected_digest[5];
    assert digest[6] = expected_digest[6];
    assert digest[7] = expected_digest[7];
    return ();
}
