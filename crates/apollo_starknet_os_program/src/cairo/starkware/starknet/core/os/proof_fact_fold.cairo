from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.cairo_blake2s.blake2s import blake_with_opcode, encode_felt252s_to_u32s
from starkware.starknet.core.os.virtual_os_output import ProofHeader

const BLAKE2S_DIGEST_N_WORDS = 8;
const PROOF_ENTRY_N_WORDS = 2 * BLAKE2S_DIGEST_N_WORDS;

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

// Writes the output digest of the two transactions' entries in the multiverifier output to
// `output_digest`.
func compute_processed_proof_output_digest{range_check_ptr}(
    left_proof_facts_size: felt,
    left_proof_facts: felt*,
    right_proof_facts_size: felt,
    right_proof_facts: felt*,
    output_digest: felt*,
) {
    alloc_locals;
    let (local multiverifier_output: felt*) = alloc();
    write_leaf_proof_entry(
        proof_facts_size=left_proof_facts_size,
        proof_facts=left_proof_facts,
        proof_entry=multiverifier_output,
    );
    write_leaf_proof_entry(
        proof_facts_size=right_proof_facts_size,
        proof_facts=right_proof_facts,
        proof_entry=&multiverifier_output[PROOF_ENTRY_N_WORDS],
    );
    blake_with_opcode(len=2 * PROOF_ENTRY_N_WORDS, data=multiverifier_output, out=output_digest);
    return ();
}

// Writes to `verification_digest` the digest the circuit verifier outputs when run on the processed
// proof of two transactions.
func compute_verification_digest{range_check_ptr}(
    left_proof_facts_size: felt,
    left_proof_facts: felt*,
    right_proof_facts_size: felt,
    right_proof_facts: felt*,
    verification_digest: felt*,
) {
    alloc_locals;
    let (local preimage: felt*) = alloc();
    write_multiverifier_circuit_hash(circuit_hash=preimage);
    compute_processed_proof_output_digest(
        left_proof_facts_size=left_proof_facts_size,
        left_proof_facts=left_proof_facts,
        right_proof_facts_size=right_proof_facts_size,
        right_proof_facts=right_proof_facts,
        output_digest=&preimage[BLAKE2S_DIGEST_N_WORDS],
    );
    blake_with_opcode(len=PROOF_ENTRY_N_WORDS, data=preimage, out=verification_digest);
    return ();
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
