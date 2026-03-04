%lang starknet

from starkware.cairo.common.cairo_builtins import HashBuiltin

@storage_var
func orchestrator_address() -> (address: felt) {
}

/// If this contract is deployed as part of the fuzz test "deploy" scenario, the orchestrator
/// address must be provided (the fuzz test will run automatically). Otherwise, deploy with [0] as
/// args.
@constructor
func constructor{syscall_ptr: felt*, pedersen_ptr: HashBuiltin*, range_check_ptr}(
    maybe_orchestrator_address: felt,
) {
    if (maybe_orchestrator_address != 0) {
        initialize(maybe_orchestrator_address);
        test_revert_fuzz();
        return ();
    }
    return ();
}

@external
func initialize{syscall_ptr: felt*, pedersen_ptr: HashBuiltin*, range_check_ptr}(
    orchestrator_address_input: felt
) {
    orchestrator_address.write(orchestrator_address_input);
    return ();
}

@external
func test_revert_fuzz{syscall_ptr: felt*, pedersen_ptr: HashBuiltin*, range_check_ptr}() {
    return ();
}
