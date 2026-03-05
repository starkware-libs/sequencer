%lang starknet

from starkware.cairo.common.cairo_builtins import HashBuiltin
from starkware.cairo.common.math import assert_not_zero
from starkware.starknet.common.syscalls import call_contract

// Scenarios.
// The RETURN scenario *must* be zero, as the zero value also indicates end of scenario stream.
const SCENARIO_RETURN = 0;
const SCENARIO_CALL = 1;

// selector_from_name("pop_front").
const POP_FRONT_SELECTOR = 0x289c2d7d6351cd03d4f928bde75fa14d5f52e32bdbc750d5296e1b48c12f1c3;
// selector_from_name("test_revert_fuzz").
const FUZZ_TEST_SELECTOR = 0x8e64dfac867f301a439703710296f437e9f91d1bba17cfea5ad7f137a5acd;

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

// Get next scenario arg from the orchestrator.
// A zero value indicates end of scenarios.
func pop_front{syscall_ptr: felt*, pedersen_ptr: HashBuiltin*, range_check_ptr}(
    address: felt
) -> felt {
    let (retdata_size: felt, retdata: felt*) = call_contract(
        contract_address=address,
        function_selector=POP_FRONT_SELECTOR,
        calldata_size=0,
        calldata=new(),
    );
    assert retdata_size = 1;
    return retdata[0];
}

@external
func test_revert_fuzz{syscall_ptr: felt*, pedersen_ptr: HashBuiltin*, range_check_ptr}() {
    alloc_locals;

    // Verify orchestrator contract is initialized.
    let (local orchestrator: felt) = orchestrator_address.read();
    assert_not_zero(orchestrator);

    // Fetch the scenario.
    local scenario = pop_front(orchestrator);

    if (scenario == SCENARIO_RETURN) {
        return ();
    }

    if (scenario == SCENARIO_CALL) {
        call_contract(
            contract_address=pop_front(orchestrator),
            function_selector=FUZZ_TEST_SELECTOR,
            calldata_size=0,
            calldata=new(),
        );
        test_revert_fuzz();
        return ();
    }

    with_attr error_message("Unknown scenario: {scenario}.") {
        assert 1 = 0;
    }
    return ();
}
