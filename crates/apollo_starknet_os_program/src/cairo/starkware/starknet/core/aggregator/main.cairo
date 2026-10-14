%builtins output pedersen range_check ecdsa bitwise ec_op keccak poseidon range_check96 add_mod mul_mod

from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.bool import FALSE, TRUE
from starkware.cairo.common.cairo_builtins import (
    BitwiseBuiltin,
    EcOpBuiltin,
    HashBuiltin,
    KeccakBuiltin,
    ModBuiltin,
    PoseidonBuiltin,
)
from starkware.starknet.core.aggregator.combine_blocks import combine_blocks
from starkware.starknet.core.os.os_config.os_config import (
    StarknetOsConfig,
    get_public_keys_hash,
    get_starknet_os_config_hash,
)
from starkware.starknet.core.os.output import (
    BLAKE2S_DIGEST_N_WORDS,
    OsCarriedOutputs,
    OsOutput,
    TxProofFacts,
    serialize_os_output,
)
from starkware.starknet.core.os.proof_fact_fold import write_proof_tree_root_entry
from starkware.starknet.core.os.verify_processed_proof import verify_processed_proof

func main{
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
}() {
    alloc_locals;

    local os_program_hash: felt;
    local n_tasks: felt;

    // Guess the Starknet OS outputs of the inner blocks.
    let (local os_outputs: OsOutput*) = alloc();

    %{ GetOsOuputForInnerBlocks %}

    // Guess whether to use KZG commitment scheme and whether to output the full state.
    local use_kzg_da;
    local full_output;
    %{ GetUseKzgDaAndFullOutputFromInput %}

    // Guess the committee's public keys.
    local public_keys: felt*;
    local n_public_keys: felt;
    %{ GetPublicKeys %}

    check_public_keys(
        n_public_keys=n_public_keys,
        public_keys=public_keys,
        starknet_os_config_hash=os_outputs[0].header.starknet_os_config_hash,
    );

    // Compute the aggregated output.
    let combined_output = combine_blocks(
        n=n_tasks,
        os_outputs=os_outputs,
        os_program_hash=os_program_hash,
        use_kzg_da=use_kzg_da,
        full_output=full_output,
    );

    let proof_facts_root_output_digest = verify_proof_facts(
        initial_carried_outputs=combined_output.initial_carried_outputs,
        final_carried_outputs=combined_output.final_carried_outputs,
    );

    // Output the bootloader output of the inner OsOutput instances.
    // This represents the "input" of the aggregator, whose correctness is later verified
    // by the bootloader by running the Cairo verifier.

    // Output the number of tasks.
    assert output_ptr[0] = n_tasks;
    let output_ptr = output_ptr + 1;

    output_blocks(
        n_tasks=n_tasks,
        os_outputs=os_outputs,
        os_program_hash=os_program_hash,
        n_public_keys=n_public_keys,
        public_keys=public_keys,
    );

    // Output the combined result. This represents the "output" of the aggregator.
    %{ GetAggregatorOutput %}

    serialize_os_output(
        os_output=combined_output,
        replace_keys_with_aliases=FALSE,
        n_public_keys=n_public_keys,
        public_keys=public_keys,
        replace_tx_proof_facts_with_root=TRUE,
        proof_facts_root_output_digest=proof_facts_root_output_digest,
    );

    %{ WriteDaSegment %}

    return ();
}

// Outputs the given OsOutput instances, with the size of the output and the program hash
// (to match the bootloader output format).
func output_blocks{
    output_ptr: felt*, range_check_ptr, ec_op_ptr: EcOpBuiltin*, poseidon_ptr: PoseidonBuiltin*
}(
    n_tasks: felt,
    os_outputs: OsOutput*,
    os_program_hash: felt,
    n_public_keys: felt,
    public_keys: felt*,
) {
    if (n_tasks == 0) {
        return ();
    }

    let output_start = output_ptr;

    // Keep a placeholder for the output size, which is computed at the end of the function.
    let output_size_placeholder = output_ptr[0];
    let output_ptr = output_ptr + 1;

    assert output_ptr[0] = os_program_hash;
    let output_ptr = output_ptr + 1;

    %{ DisableDaPageCreation %}
    serialize_os_output(
        os_output=&os_outputs[0],
        replace_keys_with_aliases=FALSE,
        n_public_keys=n_public_keys,
        public_keys=public_keys,
        replace_tx_proof_facts_with_root=FALSE,
        proof_facts_root_output_digest=cast(0, felt*),
    );

    // Compute the size of the output, including the program hash and the output size fields.
    assert output_size_placeholder = output_ptr - output_start;

    return output_blocks(
        n_tasks=n_tasks - 1,
        os_outputs=&os_outputs[1],
        os_program_hash=os_program_hash,
        n_public_keys=n_public_keys,
        public_keys=public_keys,
    );
}

// Verifies the proof of the transactions' proof facts: computes the root of their proof tree, whose
// shape the proof's prover chose, and runs the circuit verifier on the root's proof. Returns the
// output digest of the root, or zero if there are no proof facts.
func verify_proof_facts{
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
}(initial_carried_outputs: OsCarriedOutputs*, final_carried_outputs: OsCarriedOutputs*) -> felt* {
    alloc_locals;
    let n_tx_proof_facts = (
        final_carried_outputs.tx_proof_facts - initial_carried_outputs.tx_proof_facts
    ) / TxProofFacts.SIZE;
    if (n_tx_proof_facts == 0) {
        return cast(0, felt*);
    }

    // Guess the shape of the tree, which the proof's prover chose.
    local flat_tree_size: felt;
    local flat_tree: felt*;
    %{ GuessProofFactsTree %}
    let (root_entry: felt*) = alloc();
    write_proof_tree_root_entry(
        flat_tree_size=flat_tree_size,
        flat_tree=flat_tree,
        n_tx_proof_facts=n_tx_proof_facts,
        tx_proof_facts=initial_carried_outputs.tx_proof_facts,
        root_entry=root_entry,
    );

    %{ LoadCircuitVerifierTask %}
    verify_processed_proof(root_entry=root_entry);
    return &root_entry[BLAKE2S_DIGEST_N_WORDS];
}

func check_public_keys{range_check_ptr}(
    n_public_keys: felt, public_keys: felt*, starknet_os_config_hash: felt
) {
    let (public_keys_hash) = get_public_keys_hash(
        n_public_keys=n_public_keys, public_keys=public_keys
    );
    tempvar chain_id;
    tempvar fee_token_address;
    %{ GetChainIdAndFeeTokenAddressFromInput %}
    tempvar guessed_starknet_os_config = new StarknetOsConfig(
        chain_id=chain_id, fee_token_address=fee_token_address, public_keys_hash=public_keys_hash
    );
    let (guessed_starknet_os_config_hash) = get_starknet_os_config_hash(
        starknet_os_config=guessed_starknet_os_config
    );
    assert guessed_starknet_os_config_hash = starknet_os_config_hash;
    return ();
}
