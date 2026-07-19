from starkware.cairo.common.cairo_builtins import HashBuiltin
from starkware.cairo.common.ec import StarkCurve
from starkware.cairo.common.hash_state import (
    hash_finalize,
    hash_init,
    hash_update_single,
    hash_update_with_hashchain,
)
from starkware.cairo.common.math import assert_le_felt
from starkware.cairo.common.math_cmp import is_le_felt
from starkware.starknet.common.storage import ADDR_BOUND, normalize_address
from starkware.starknet.core.os.hash.hash_state_blake import HashState as BlakeHashState
from starkware.starknet.core.os.hash.hash_state_blake import hash_finalize as hash_finalize_blake
from starkware.starknet.core.os.hash.hash_state_blake import hash_init as hash_init_blake
from starkware.starknet.core.os.hash.hash_state_blake import (
    hash_update_single as hash_update_single_blake,
)
from starkware.starknet.core.os.hash.hash_state_blake import (
    hash_update_with_nested_hash as hash_update_with_nested_hash_blake,
)

const CONTRACT_ADDRESS_PREFIX = 'STARKNET_CONTRACT_ADDRESS';

func get_contract_address{hash_ptr: HashBuiltin*, range_check_ptr}(
    salt: felt,
    class_hash: felt,
    constructor_calldata_size: felt,
    constructor_calldata: felt*,
    deployer_address: felt,
) -> (contract_address: felt) {
    let (hash_state_ptr) = hash_init();
    let (hash_state_ptr) = hash_update_single(
        hash_state_ptr=hash_state_ptr, item=CONTRACT_ADDRESS_PREFIX
    );
    let (hash_state_ptr) = hash_update_single(hash_state_ptr=hash_state_ptr, item=deployer_address);
    let (hash_state_ptr) = hash_update_single(hash_state_ptr=hash_state_ptr, item=salt);
    let (hash_state_ptr) = hash_update_single(hash_state_ptr=hash_state_ptr, item=class_hash);
    let (hash_state_ptr) = hash_update_with_hashchain(
        hash_state_ptr=hash_state_ptr,
        data_ptr=constructor_calldata,
        data_length=constructor_calldata_size,
    );
    let (contract_address_before_modulo) = hash_finalize(hash_state_ptr=hash_state_ptr);
    let (contract_address) = normalize_address(addr=contract_address_before_modulo);

    return (contract_address=contract_address);
}

// Same as `get_contract_address`, but uses Blake2s (the optimized
// `encode_felt252_data_and_calc_blake_hash` encoding) instead of Pedersen.
func get_contract_address_blake{range_check_ptr}(
    salt: felt,
    class_hash: felt,
    constructor_calldata_size: felt,
    constructor_calldata: felt*,
    deployer_address: felt,
) -> (contract_address: felt) {
    let hash_state: BlakeHashState = hash_init_blake();
    with hash_state {
        hash_update_single_blake(item=CONTRACT_ADDRESS_PREFIX);
        hash_update_single_blake(item=deployer_address);
        hash_update_single_blake(item=salt);
        hash_update_single_blake(item=class_hash);
        hash_update_with_nested_hash_blake(
            data_ptr=constructor_calldata, data_length=constructor_calldata_size
        );
    }
    let contract_address_before_modulo: felt = hash_finalize_blake(hash_state=hash_state);
    let (contract_address) = normalize_address(addr=contract_address_before_modulo);

    return (contract_address=contract_address);
}

// A fixed quadratic non-residue in the STARK field: `t = NON_RESIDUE_WITNESS_FACTOR * s * s`
// proves `t` is a non-residue.
const NON_RESIDUE_WITNESS_FACTOR = 3;
// Addresses below this bound (the field prime minus ADDR_BOUND) have a second lift into the
// field: both `address` and `address + ADDR_BOUND` are below the prime.
const ADDRESS_SECOND_LIFT_BOUND = 0x11000000000000000000000000000000000000000000000101;

// Returns `x^3 + ALPHA * x + BETA` — a square iff `x` is a STARK-curve x-coordinate, i.e. iff
// some Pedersen hash output equals `x`.
func curve_cubic(x: felt) -> felt {
    return x * x * x + StarkCurve.ALPHA * x + StarkCurve.BETA;
}

// Increments `candidate` (expected ~1 step) until no lift of it is a STARK-curve x-coordinate,
// so no Pedersen derivation can produce the returned address and funded-but-undeployed Blake
// addresses cannot be front-run through the Pedersen deploy paths.
//
// Each skipped candidate is proven reachable with an on-curve square-root witness; the returned
// candidate is proven unreachable with non-residue witnesses (`witness^2 = t / 3`, sound since 3
// is a non-residue). The hint only supplies witnesses — the escape logic itself is fully
// verified.
//
// Note: the Rust derivation also wraps around ADDR_BOUND and skips addresses < 2. Both cases
// require a Blake output within ~2^-240 of the bound edges, which is cryptographically
// unreachable; hitting one would make the transaction unprovable rather than diverge.
func escape_pedersen_image{range_check_ptr}(candidate: felt) -> (contract_address: felt) {
    alloc_locals;
    local is_reachable;
    local reachable_lift;
    local witness;
    local second_witness;
    %{ EscapePedersenImageWitness %}
    assert is_reachable * (is_reachable - 1) = 0;
    assert reachable_lift * (reachable_lift - 1) = 0;

    if (is_reachable != 0) {
        // The candidate is in the Pedersen image: verify with an on-curve square-root witness
        // for one of its lifts, then move to the next candidate.
        if (reachable_lift != 0) {
            // A second lift exists only below ADDRESS_SECOND_LIFT_BOUND.
            assert_le_felt(candidate, ADDRESS_SECOND_LIFT_BOUND - 1);
            let t_lift = curve_cubic(x=candidate + ADDR_BOUND);
            assert witness * witness = t_lift;
            return escape_pedersen_image(candidate=candidate + 1);
        }
        let t = curve_cubic(x=candidate);
        assert witness * witness = t;
        return escape_pedersen_image(candidate=candidate + 1);
    }

    // The candidate escapes the Pedersen image: every lift of it is off-curve.
    let t = curve_cubic(x=candidate);
    assert witness * witness = t / NON_RESIDUE_WITNESS_FACTOR;
    let candidate_has_second_lift = is_le_felt(candidate, ADDRESS_SECOND_LIFT_BOUND - 1);
    if (candidate_has_second_lift != 0) {
        let t_lift = curve_cubic(x=candidate + ADDR_BOUND);
        assert second_witness * second_witness = t_lift / NON_RESIDUE_WITNESS_FACTOR;
        return (contract_address=candidate);
    }
    return (contract_address=candidate);
}

// The deploy-account v4 / deploy_v2 contract address derivation: the Blake2s hash of the
// deployment arguments, escaped out of the Pedersen image.
func get_contract_address_blake_escaped{range_check_ptr}(
    salt: felt,
    class_hash: felt,
    constructor_calldata_size: felt,
    constructor_calldata: felt*,
    deployer_address: felt,
) -> (contract_address: felt) {
    let (raw_address) = get_contract_address_blake(
        salt=salt,
        class_hash=class_hash,
        constructor_calldata_size=constructor_calldata_size,
        constructor_calldata=constructor_calldata,
        deployer_address=deployer_address,
    );
    return escape_pedersen_image(candidate=raw_address);
}
