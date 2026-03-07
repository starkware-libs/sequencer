%lang starknet

from starkware.cairo.common.cairo_builtins import HashBuiltin
from starkware.cairo.common.math import assert_not_zero
from starkware.starknet.common.messages import send_message_to_l1
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
const SCENARIO_PANIC = 6;
const SCENARIO_INCREMENT_COUNTER = 7;
const SCENARIO_SEND_MESSAGE = 8;
const SCENARIO_DEPLOY_NON_EXISTING = 9;
const SCENARIO_LIBRARY_CALL_NON_EXISTING = 10;
const SCENARIO_SHA256 = 11;
const SCENARIO_KECCAK = 12;
const SCENARIO_CALL_UNDEPLOYED = 13;

// selector_from_name("pop_front").
const POP_FRONT_SELECTOR = 0x289c2d7d6351cd03d4f928bde75fa14d5f52e32bdbc750d5296e1b48c12f1c3;

@storage_var
func orchestrator_address() -> (address: felt) {
}

@storage_var
func counter() -> (value: felt) {
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
    counter.write(0xc00);
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
        let address = pop_front(orchestrator);
        let selector = pop_front(orchestrator);
        let _should_unwrap = pop_front(orchestrator);
        call_contract(
            contract_address=address, function_selector=selector, calldata_size=0, calldata=new(),
        );
        test_revert_fuzz();
        return ();
    }

    if (scenario == SCENARIO_LIBRARY_CALL) {
        let class_hash = pop_front(orchestrator);
        let selector = pop_front(orchestrator);
        let _should_unwrap = pop_front(orchestrator);
        library_call(
            class_hash=class_hash, function_selector=selector, calldata_size=0, calldata=new(),
        );
        test_revert_fuzz();
        return ();
    }

    if (scenario == SCENARIO_WRITE) {
        let key = pop_front(orchestrator);
        let value = pop_front(orchestrator);
        storage_write(address=key, value=value);
        test_revert_fuzz();
        return ();
    }

    if (scenario == SCENARIO_REPLACE_CLASS) {
        replace_class(class_hash=pop_front(orchestrator));
        test_revert_fuzz();
        return ();
    }

    if (scenario == SCENARIO_DEPLOY) {
        // The class hash is assumed to be a fuzz test class hash.
        // Deploy it with a non-trivial orchestrator address.
        let class_hash = pop_front(orchestrator);
        let salt = pop_front(orchestrator);
        local ctor_calldata: felt* = new(orchestrator);
        deploy(
            class_hash=class_hash,
            contract_address_salt=salt,
            constructor_calldata_size=1,
            constructor_calldata=ctor_calldata,
            deploy_from_zero=1,
        );
        test_revert_fuzz();
        return ();
    }

    if (scenario == SCENARIO_PANIC) {
        // Cairo0 panics cannot be caught, so no need to handle orchestrator index.
        with_attr error_message("panic_scenario") {
            assert 0 = 1;
        }
        test_revert_fuzz();
        return ();
    }

    if (scenario == SCENARIO_INCREMENT_COUNTER) {
        let (value) = counter.read();
        counter.write(value + 1);
        test_revert_fuzz();
        return ();
    }

    if (scenario == SCENARIO_SEND_MESSAGE) {
        local payload: felt* = new(pop_front(orchestrator));
        send_message_to_l1(to_address=0xadd0, payload_size=1, payload=payload);
        test_revert_fuzz();
        return ();
    }

    if (scenario == SCENARIO_DEPLOY_NON_EXISTING) {
        let class_hash = 0xde6107000c0;
        let salt = 0;
        deploy(
            class_hash=class_hash,
            contract_address_salt=salt,
            constructor_calldata_size=0,
            constructor_calldata=new(),
            deploy_from_zero=1
        );
        // Should always fail anyway, no need to recurse.
        return ();
    }

    if (scenario == SCENARIO_LIBRARY_CALL_NON_EXISTING) {
        let class_hash = 0x11bca11000c0;
        library_call(class_hash=class_hash, function_selector=0, calldata_size=0, calldata=new());
        // Should always fail anyway, no need to recurse.
        return ();
    }

    if ((scenario - SCENARIO_SHA256) * (scenario - SCENARIO_KECCAK) == 0) {
        // Not supported in Cairo0.
        with_attr error_message("new_hash_cairo0") {
            assert 0 = 1;
        }
        return ();
    }

    if (scenario == SCENARIO_CALL_UNDEPLOYED) {
        let address = pop_front(orchestrator);
        let selector = pop_front(orchestrator);
        let _should_unwrap = pop_front(orchestrator);
        call_contract(
            contract_address=address, function_selector=selector, calldata_size=0, calldata=new()
        );
        // Calling an undeployed contract should be an uncatchable fail.
        with_attr error_message("should_fail_undeployed") {
            assert 0 = 1;
        }
        return ();
    }

    with_attr error_message("Unknown scenario: {scenario}.") {
        assert 1 = 0;
    }
    return ();
}
