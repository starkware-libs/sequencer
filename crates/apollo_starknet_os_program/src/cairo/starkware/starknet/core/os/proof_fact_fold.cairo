from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.cairo_blake2s.blake2s import blake_with_opcode, encode_felt252s_to_u32s
from starkware.cairo.common.memcpy import memcpy
from starkware.cairo.common.registers import get_label_location

const BLAKE2S_DIGEST_N_WORDS = 8;
const PROOF_ENTRY_N_WORDS = 2 * BLAKE2S_DIGEST_N_WORDS;

// Computes the output digest of a processed proof covering two verified proofs: the blake2s
// digest of the two proofs' entries in the multiverifier output. A single transaction's proof
// fills both of the multiverifier's verifier slots, so its callers pass the same proof facts
// twice.
func compute_processed_proof_output_digest{range_check_ptr}(
    first_proof_facts_size: felt,
    first_proof_facts: felt*,
    second_proof_facts_size: felt,
    second_proof_facts: felt*,
) -> (output_digest: felt*) {
    alloc_locals;
    let (local preimage: felt*) = alloc();
    write_proof_entry(
        proof_entry=preimage,
        proof_facts_size=first_proof_facts_size,
        proof_facts=first_proof_facts,
    );
    write_proof_entry(
        proof_entry=preimage + PROOF_ENTRY_N_WORDS,
        proof_facts_size=second_proof_facts_size,
        proof_facts=second_proof_facts,
    );
    let (local output_digest: felt*) = alloc();
    blake_with_opcode(len=2 * PROOF_ENTRY_N_WORDS, data=preimage, out=output_digest);
    return (output_digest=output_digest);
}

// Writes a verified proof's entry in the multiverifier output: the leaf verifier's circuit hash
// followed by the proof's leaf output digest.
func write_proof_entry{range_check_ptr}(
    proof_entry: felt*, proof_facts_size: felt, proof_facts: felt*
) {
    alloc_locals;
    let (leaf_verifier_circuit_hash) = get_leaf_verifier_circuit_hash();
    memcpy(dst=proof_entry, src=leaf_verifier_circuit_hash, len=BLAKE2S_DIGEST_N_WORDS);
    let (proof_output_digest) = compute_leaf_output_digest(
        proof_facts_size=proof_facts_size, proof_facts=proof_facts
    );
    memcpy(
        dst=proof_entry + BLAKE2S_DIGEST_N_WORDS,
        src=proof_output_digest,
        len=BLAKE2S_DIGEST_N_WORDS,
    );
    return ();
}

func compute_verification_digest{range_check_ptr}(processed_proof_output_digest: felt*) -> (
    verification_digest: felt*
) {
    alloc_locals;
    let (local preimage: felt*) = alloc();
    let (multiverifier_circuit_hash) = get_multiverifier_circuit_hash();
    memcpy(dst=preimage, src=multiverifier_circuit_hash, len=BLAKE2S_DIGEST_N_WORDS);
    memcpy(
        dst=preimage + BLAKE2S_DIGEST_N_WORDS,
        src=processed_proof_output_digest,
        len=BLAKE2S_DIGEST_N_WORDS,
    );
    let (local verification_digest: felt*) = alloc();
    blake_with_opcode(len=PROOF_ENTRY_N_WORDS, data=preimage, out=verification_digest);
    return (verification_digest=verification_digest);
}

func pack_output_digest(output_digest: felt*) -> (low: felt, high: felt) {
    return (
        low=output_digest[0] + output_digest[1] * (2 ** 32) + output_digest[2] * (2 ** 64) +
        output_digest[3] * (2 ** 96),
        high=output_digest[4] + output_digest[5] * (2 ** 32) + output_digest[6] * (2 ** 64) +
        output_digest[7] * (2 ** 96),
    );
}

func compute_leaf_output_digest{range_check_ptr}(proof_facts_size: felt, proof_facts: felt*) -> (
    output_digest: felt*
) {
    alloc_locals;
    let (local encoded_words: felt*) = alloc();
    let encoded_words_len = encode_felt252s_to_u32s(
        packed_values_len=proof_facts_size - 2,
        packed_values=proof_facts + 2,
        unpacked_u32s=encoded_words,
    );
    let (local output_digest: felt*) = alloc();
    blake_with_opcode(len=encoded_words_len, data=encoded_words, out=output_digest);
    return (output_digest=output_digest);
}

func get_leaf_verifier_circuit_hash() -> (circuit_hash: felt*) {
    let (circuit_hash) = get_label_location(leaf_verifier_circuit_hash);
    return (circuit_hash=circuit_hash);

    leaf_verifier_circuit_hash:
    dw 0xd2d85a42;
    dw 0x79697b22;
    dw 0x3a41a061;
    dw 0x011cb393;
    dw 0x7a040ec9;
    dw 0x4508f4ca;
    dw 0x42239409;
    dw 0x60f3baea;
}

func get_multiverifier_circuit_hash() -> (circuit_hash: felt*) {
    let (circuit_hash) = get_label_location(multiverifier_circuit_hash);
    return (circuit_hash=circuit_hash);

    multiverifier_circuit_hash:
    dw 0xa5989715;
    dw 0x2377c07a;
    dw 0xc6d1e844;
    dw 0x54f0a04d;
    dw 0x8be65a7d;
    dw 0xfd73c261;
    dw 0x9078e728;
    dw 0x973f680f;
}
