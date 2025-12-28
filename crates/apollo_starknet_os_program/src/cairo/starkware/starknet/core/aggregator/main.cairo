%builtins output pedersen range_check ec_op poseidon

from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.bool import FALSE
from starkware.cairo.common.cairo_builtins import EcOpBuiltin, HashBuiltin, PoseidonBuiltin
from starkware.cairo.common.ec_point import EcPoint
from starkware.starknet.core.aggregator.combine_blocks import combine_blocks
from starkware.starknet.core.os.os_config.os_config import (
    StarknetOsConfig,
    get_public_keys_hash,
    get_starknet_os_config_hash,
)
from starkware.starknet.core.os.output import OsOutput, serialize_os_output

func main{
    output_ptr: felt*,
    pedersen_ptr: HashBuiltin*,
    range_check_ptr,
    ec_op_ptr: EcOpBuiltin*,
    poseidon_ptr: PoseidonBuiltin*,
}() {
    alloc_locals;

    local os_program_hash: felt;
    local n_tasks: felt;

    // Guess the Starknet OS outputs of the inner blocks.
    let (local os_outputs: OsOutput*) = alloc();

    %{ GetOsOuputForInnerBlocks %}

    // Guess whether to use KZG commitment scheme and whether to output the full state.
    tempvar use_kzg_da = nondet %{ program_input["use_kzg_da"] %};
    tempvar full_output = nondet %{ program_input["full_output"] %};

    // Guess the committee's public keys.
    local public_keys: felt*;
    local n_public_keys: felt;
    %{ GetPublicKeysFromAggregatorInput %}

    check_public_keys{hash_ptr=pedersen_ptr}(
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

func check_public_keys{hash_ptr: HashBuiltin*}(
    n_public_keys: felt, public_keys: felt*, starknet_os_config_hash: felt
) {
    let (public_keys_hash) = get_public_keys_hash(
        n_public_keys=n_public_keys, public_keys=public_keys
    );
    tempvar chain_id = nondet %{ program_input["chain_id"] %};
    tempvar fee_token_address = nondet %{ program_input["fee_token_address"] %};
    tempvar guessed_starknet_os_config = new StarknetOsConfig(
        chain_id=chain_id, fee_token_address=fee_token_address, public_keys_hash=public_keys_hash
    );
    let (guessed_starknet_os_config_hash) = get_starknet_os_config_hash(
        starknet_os_config=guessed_starknet_os_config
    );
    assert guessed_starknet_os_config_hash = starknet_os_config_hash;
    return ();
}
