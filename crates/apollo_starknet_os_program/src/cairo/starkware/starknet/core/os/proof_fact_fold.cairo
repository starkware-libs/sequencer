from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.cairo_blake2s.blake2s import blake_with_opcode, encode_felt252s_to_u32s
from starkware.cairo.common.memcpy import memcpy
from starkware.starknet.core.os.virtual_os_output import ProofHeader

const BLAKE2S_DIGEST_N_WORDS = 8;
const PROOF_ENTRY_N_WORDS = 2 * BLAKE2S_DIGEST_N_WORDS;

// The proof facts of a transaction.
struct ProofFactsReference {
    proof_facts_size: felt,
    proof_facts: felt*,
}

// Appends the transaction's proof facts to `proof_facts_references`, if it has any.
func record_proof_facts_reference{proof_facts_references: ProofFactsReference*}(
    proof_facts_size: felt, proof_facts: felt*
) {
    if (proof_facts_size == 0) {
        return ();
    }
    assert [proof_facts_references] = ProofFactsReference(
        proof_facts_size=proof_facts_size, proof_facts=proof_facts
    );
    let proof_facts_references = &proof_facts_references[1];
    return ();
}

// Returns the number of a block's transactions with proof facts, given their references, and the
// packed output digest of their processed proof. A block supports at most one such transaction.
func single_processed_proof_output{range_check_ptr}(
    proof_facts_references_start: ProofFactsReference*,
    proof_facts_references_end: ProofFactsReference*,
) -> (
    n_proof_facts_transactions: felt,
    processed_proof_output_low: felt,
    processed_proof_output_high: felt,
) {
    alloc_locals;
    if (proof_facts_references_end == proof_facts_references_start) {
        return (
            n_proof_facts_transactions=0,
            processed_proof_output_low=0,
            processed_proof_output_high=0,
        );
    }
    with_attr error_message("A block supports at most one transaction with proof facts.") {
        assert proof_facts_references_end = &proof_facts_references_start[1];
    }
    let (leaf_digest) = compute_leaf_output_digest(
        proof_facts_size=proof_facts_references_start.proof_facts_size,
        proof_facts=proof_facts_references_start.proof_facts,
    );
    // The single transaction's proof fills both of the multiverifier's verifier slots.
    let (output_digest) = combine_leaf_digests(
        left_leaf_digest=leaf_digest, right_leaf_digest=leaf_digest
    );
    let (processed_proof_output_low, processed_proof_output_high) = pack_output_digest(
        output_digest=output_digest
    );
    return (
        n_proof_facts_transactions=1,
        processed_proof_output_low=processed_proof_output_low,
        processed_proof_output_high=processed_proof_output_high,
    );
}

// Computes a transaction's leaf output digest: the blake2s digest of its proof facts, starting at
// the program hash, encoded as u32s. The preimage drops the proof version and variant markers.
// Assumption: `proof_facts_size` is at least `ProofHeader.SIZE`.
func compute_leaf_output_digest{range_check_ptr}(proof_facts_size: felt, proof_facts: felt*) -> (
    output_digest: felt*
) {
    alloc_locals;
    let (local encoded_words: felt*) = alloc();
    let encoded_words_len = encode_felt252s_to_u32s(
        packed_values_len=proof_facts_size - ProofHeader.program_hash,
        packed_values=&proof_facts[ProofHeader.program_hash],
        unpacked_u32s=encoded_words,
    );
    let (local output_digest: felt*) = alloc();
    blake_with_opcode(len=encoded_words_len, data=encoded_words, out=output_digest);
    return (output_digest=output_digest);
}

// Combines two leaf digests into their parent's output digest: the blake2s digest of the two
// leaves' entries in the multiverifier output, each being the leaf verifier's circuit hash
// followed by the leaf digest.
func combine_leaf_digests{range_check_ptr}(left_leaf_digest: felt*, right_leaf_digest: felt*) -> (
    output_digest: felt*
) {
    alloc_locals;
    let (local leaf_verifier_circuit_hash: felt*) = get_leaf_verifier_circuit_hash();
    let (local preimage: felt*) = alloc();
    memcpy(dst=preimage, src=leaf_verifier_circuit_hash, len=BLAKE2S_DIGEST_N_WORDS);
    memcpy(dst=&preimage[BLAKE2S_DIGEST_N_WORDS], src=left_leaf_digest, len=BLAKE2S_DIGEST_N_WORDS);
    memcpy(
        dst=&preimage[PROOF_ENTRY_N_WORDS],
        src=leaf_verifier_circuit_hash,
        len=BLAKE2S_DIGEST_N_WORDS,
    );
    memcpy(
        dst=&preimage[PROOF_ENTRY_N_WORDS + BLAKE2S_DIGEST_N_WORDS],
        src=right_leaf_digest,
        len=BLAKE2S_DIGEST_N_WORDS,
    );
    let (local output_digest: felt*) = alloc();
    blake_with_opcode(len=2 * PROOF_ENTRY_N_WORDS, data=preimage, out=output_digest);
    return (output_digest=output_digest);
}

// Computes the digest the circuit verifier outputs when run on a processed proof: the blake2s
// digest of the multiverifier's circuit hash followed by the processed proof's output digest.
func compute_verification_digest{range_check_ptr}(processed_proof_output_digest: felt*) -> (
    verification_digest: felt*
) {
    alloc_locals;
    let (local multiverifier_circuit_hash: felt*) = get_multiverifier_circuit_hash();
    let (local preimage: felt*) = alloc();
    memcpy(dst=preimage, src=multiverifier_circuit_hash, len=BLAKE2S_DIGEST_N_WORDS);
    memcpy(
        dst=&preimage[BLAKE2S_DIGEST_N_WORDS],
        src=processed_proof_output_digest,
        len=BLAKE2S_DIGEST_N_WORDS,
    );
    let (local verification_digest: felt*) = alloc();
    blake_with_opcode(len=PROOF_ENTRY_N_WORDS, data=preimage, out=verification_digest);
    return (verification_digest=verification_digest);
}

// Packs a digest of eight little-endian 32-bit words into two 128-bit felts: `low` holds words
// 0-3 and `high` holds words 4-7, each in little-endian order.
func pack_output_digest(output_digest: felt*) -> (low: felt, high: felt) {
    return (
        low=output_digest[3] * 2 ** 96 + output_digest[2] * 2 ** 64 + output_digest[1] * 2 ** 32 +
        output_digest[0],
        high=output_digest[7] * 2 ** 96 + output_digest[6] * 2 ** 64 + output_digest[5] * 2 ** 32 +
        output_digest[4],
    );
}

// Returns the leaf verifier's circuit hash as eight little-endian 32-bit words.
// TODO(Einat): accept the leaf verifiers of all the registry's trace sizes.
func get_leaf_verifier_circuit_hash() -> (circuit_hash: felt*) {
    tempvar circuit_hash: felt* = new (
        0xd2d85a42,
        0x79697b22,
        0x3a41a061,
        0x011cb393,
        0x7a040ec9,
        0x4508f4ca,
        0x42239409,
        0x60f3baea,
    );
    return (circuit_hash=circuit_hash);
}

// Returns the multiverifier's circuit hash as eight little-endian 32-bit words.
func get_multiverifier_circuit_hash() -> (circuit_hash: felt*) {
    tempvar circuit_hash: felt* = new (
        0xa5989715,
        0x2377c07a,
        0xc6d1e844,
        0x54f0a04d,
        0x8be65a7d,
        0xfd73c261,
        0x9078e728,
        0x973f680f,
    );
    return (circuit_hash=circuit_hash);
}
