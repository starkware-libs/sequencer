%lang starknet

from starkware.cairo.common.bool import TRUE
from starkware.cairo.common.cairo_builtins import HashBuiltin
from starkware.cairo.common.math import assert_not_zero
from starkware.starknet.common.syscalls import (
    call_contract,
    deploy,
    library_call,
    replace_class,
    storage_write,
)

// Scenarios.
// The RETURN scenario *must* be zero, as the zero value also indicates end of scenario stream.
const SCENARIO_RETURN = 0;
const SCENARIO_CALL = 1;
const SCENARIO_LIBRARY_CALL = 2;
const SCENARIO_WRITE = 3;
const SCENARIO_REPLACE_CLASS = 4;
const SCENARIO_DEPLOY = 5;

// selector_from_name("pop_front").
const POP_FRONT_SELECTOR = 0x289c2d7d6351cd03d4f928bde75fa14d5f52e32bdbc750d5296e1b48c12f1c3;
// selector_from_name("test_revert_fuzz").
const FUZZ_TEST_SELECTOR = 0x8e64dfac867f301a439703710296f437e9f91d1bba17cfea5ad7f137a5acd;

@storage_var
func orchestrator_address() -> (address: felt) {
}

/// If this contract is deployed as part of the fuzz test "deploy" scenario, the orchestrator
/// address must be provided, and run_fuzz must be non zero. Otherwise, deploy with [0,0] as args.
@constructor
func constructor{syscall_ptr: felt*, pedersen_ptr: HashBuiltin*, range_check_ptr}(
    maybe_orchestrator_address: felt, run_fuzz: felt,
) {
    if (maybe_orchestrator_address != 0) {
        initialize(maybe_orchestrator_address);
        tempvar syscall_ptr = syscall_ptr;
        tempvar pedersen_ptr = pedersen_ptr;
        tempvar range_check_ptr = range_check_ptr;
    } else {
        tempvar syscall_ptr = syscall_ptr;
        tempvar pedersen_ptr = pedersen_ptr;
        tempvar range_check_ptr = range_check_ptr;
    }
    if (run_fuzz != 0) {
        test_revert_fuzz();
        tempvar syscall_ptr = syscall_ptr;
        tempvar pedersen_ptr = pedersen_ptr;
        tempvar range_check_ptr = range_check_ptr;
    } else {
        tempvar syscall_ptr = syscall_ptr;
        tempvar pedersen_ptr = pedersen_ptr;
        tempvar range_check_ptr = range_check_ptr;
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
    } else {
        tempvar syscall_ptr = syscall_ptr;
        tempvar pedersen_ptr = pedersen_ptr;
        tempvar range_check_ptr = range_check_ptr;
    }

    if (scenario == SCENARIO_CALL) {
        call_contract(
            contract_address=pop_front(orchestrator),
            function_selector=FUZZ_TEST_SELECTOR,
            calldata_size=0,
            calldata=new(),
        );
        tempvar syscall_ptr = syscall_ptr;
        tempvar pedersen_ptr = pedersen_ptr;
        tempvar range_check_ptr = range_check_ptr;
    } else {
        tempvar syscall_ptr = syscall_ptr;
        tempvar pedersen_ptr = pedersen_ptr;
        tempvar range_check_ptr = range_check_ptr;
    }

    if (scenario == SCENARIO_LIBRARY_CALL) {
        library_call(
            class_hash=pop_front(orchestrator),
            function_selector=FUZZ_TEST_SELECTOR,
            calldata_size=0,
            calldata=new(),
        );
        tempvar syscall_ptr = syscall_ptr;
        tempvar pedersen_ptr = pedersen_ptr;
        tempvar range_check_ptr = range_check_ptr;
    } else {
        tempvar syscall_ptr = syscall_ptr;
        tempvar pedersen_ptr = pedersen_ptr;
        tempvar range_check_ptr = range_check_ptr;
    }

    if (scenario == SCENARIO_WRITE) {
        let key = pop_front(orchestrator);
        let value = pop_front(orchestrator);
        storage_write(address=key, value=value);
        tempvar syscall_ptr = syscall_ptr;
        tempvar pedersen_ptr = pedersen_ptr;
        tempvar range_check_ptr = range_check_ptr;
    } else {
        tempvar syscall_ptr = syscall_ptr;
        tempvar pedersen_ptr = pedersen_ptr;
        tempvar range_check_ptr = range_check_ptr;
    }

    if (scenario == SCENARIO_REPLACE_CLASS) {
        replace_class(class_hash=pop_front(orchestrator));
        tempvar syscall_ptr = syscall_ptr;
        tempvar pedersen_ptr = pedersen_ptr;
        tempvar range_check_ptr = range_check_ptr;
    } else {
        tempvar syscall_ptr = syscall_ptr;
        tempvar pedersen_ptr = pedersen_ptr;
        tempvar range_check_ptr = range_check_ptr;
    }

    if (scenario == SCENARIO_DEPLOY) {
        // The class hash is assumed to be a fuzz test class hash.
        // Deploy it with a non-trivial orchestrator address.
        let class_hash = pop_front(orchestrator);
        let salt = pop_front(orchestrator);
        local ctor_calldata: felt* = new(orchestrator, TRUE);
        deploy(
            class_hash=class_hash,
            contract_address_salt=salt,
            constructor_calldata_size=2,
            constructor_calldata=ctor_calldata,
            deploy_from_zero=1,
        );
        tempvar syscall_ptr = syscall_ptr;
        tempvar pedersen_ptr = pedersen_ptr;
        tempvar range_check_ptr = range_check_ptr;
    } else {
        tempvar syscall_ptr = syscall_ptr;
        tempvar pedersen_ptr = pedersen_ptr;
        tempvar range_check_ptr = range_check_ptr;
    }

    test_revert_fuzz();
    return ();
}
