%lang starknet

from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.bitwise import bitwise_xor
from starkware.cairo.common.bool import FALSE
from starkware.cairo.common.cairo_builtins import BitwiseBuiltin, HashBuiltin, EcOpBuiltin
from starkware.cairo.common.ec import ec_op
from starkware.cairo.common.ec_point import EcPoint
from starkware.cairo.common.memcpy import memcpy
from starkware.cairo.common.registers import get_fp_and_pc
from starkware.starknet.common.messages import send_message_to_l1
from starkware.starknet.common.syscalls import (
    TxInfo,
    storage_read,
    storage_write,
    library_call,
    library_call_l1_handler,
    deploy,
    call_contract,
    get_block_number,
    get_block_timestamp,
    get_caller_address,
    get_contract_address as get_contract_address_syscall,
    get_sequencer_address,
    replace_class,
    get_tx_info,
    get_tx_signature,
    emit_event,
)
from starkware.starknet.core.os.contract_address.contract_address import get_contract_address

// selector_from_name('transferFrom').
const TRANSFER_FROM_SELECTOR = 0x0041b033f4a31df8067c24d1e9b550a2ce75fd4a29e1147af9752174f0e6cb20;

@storage_var
func number_map(key: felt) -> (value: felt) {
}

@constructor
func constructor{syscall_ptr: felt*}(address: felt, value: felt) {
    storage_write(address=address, value=value);
    return ();
}

@external
func without_arg() {
    return ();
}

@external
func with_arg(num: felt) {
    assert num = 25;
    return ();
}

@external
func return_result(num: felt) -> (result: felt) {
    return (result=num);
}

// Not intended to be called - just so all syscall hints appear in the compiled contract.
func other_syscalls{syscall_ptr: felt*}() {
    library_call_l1_handler(
        class_hash=1,
        function_selector=2,
        calldata_size=3,
        calldata=new(4, 5, 6),
    );
    get_contract_address_syscall();
    return ();
}

@external
func bitwise_and{bitwise_ptr: BitwiseBuiltin*}(x: felt, y: felt) {
    bitwise_ptr.x = x;
    bitwise_ptr.y = y;
    let x_and_y = bitwise_ptr.x_and_y;
    let x_xor_y = bitwise_ptr.x_xor_y;
    let x_or_y = bitwise_ptr.x_or_y;
    let bitwise_ptr = bitwise_ptr + BitwiseBuiltin.SIZE;
    assert x_and_y = 15;
    return ();
}

@external
func sqrt{range_check_ptr}(value: felt) {
    alloc_locals;
    local root: felt;

    %{
        from starkware.python.math_utils import isqrt
        value = ids.value % PRIME
        assert value < 2 ** 250, f"value={value} is outside of the range [0, 2**250)."
        assert 2 ** 250 < PRIME
        ids.root = isqrt(value)
    %}

    assert root = 9;
    return ();
}

@external
func test_storage_read_write{syscall_ptr: felt*}(address: felt, value: felt) -> (result: felt) {
    storage_write(address=address, value=value);
    let (read_value) = storage_read(address=address);
    return (result=read_value);
}

@external
func write_a_lot{syscall_ptr: felt*}(n_writes: felt, value: felt) {
    if (n_writes == 0) {
        return ();
    }
    send_message(7);
    storage_write(address=n_writes, value=value);
    return write_a_lot(n_writes - 1, value);
}

@external
func write_and_revert{syscall_ptr: felt*}(address: felt, value: felt) {
    storage_write(address=address, value=value);
    assert 0 = 1;
    return ();
}

@external
func test_long_retdata() -> (a: felt, b: felt, c: felt, d: felt, e: felt) {
    return (a=0, b=1, c=2, d=3, e=4);
}

@external
@raw_output
func test_library_call{syscall_ptr: felt*}(
    class_hash: felt, selector: felt, calldata_len: felt, calldata: felt*
) -> (retdata_size: felt, retdata: felt*) {
    let (retdata_size: felt, retdata: felt*) = library_call(
        class_hash=class_hash,
        function_selector=selector,
        calldata_size=calldata_len,
        calldata=calldata,
    );
    return (retdata_size=retdata_size, retdata=retdata);
}

@external
func test_nested_library_call{syscall_ptr: felt*}(
    class_hash: felt, lib_selector: felt, nested_selector: felt, calldata_len: felt, calldata: felt*
) -> (result: felt) {
    alloc_locals;
    assert calldata_len = 2;
    local nested_library_calldata: felt* = new(
        class_hash, nested_selector, 2, calldata[0] + 1, calldata[1] + 1
    );
    let (retdata_size: felt, retdata: felt*) = library_call(
        class_hash=class_hash,
        function_selector=lib_selector,
        calldata_size=5,
        calldata=nested_library_calldata,
    );

    let (retdata_size: felt, retdata: felt*) = library_call(
        class_hash=class_hash,
        function_selector=nested_selector,
        calldata_size=calldata_len,
        calldata=calldata,
    );

    return (result=0);
}

@external
@raw_output
func test_call_contract{syscall_ptr: felt*}(
    contract_address: felt, function_selector: felt, calldata_len: felt, calldata: felt*
) -> (retdata_size: felt, retdata: felt*) {
    let (retdata_size: felt, retdata: felt*) = call_contract(
        contract_address=contract_address,
        function_selector=function_selector,
        calldata_size=calldata_len,
        calldata=calldata,
    );
    return (retdata_size=retdata_size, retdata=retdata);
}

@external
func call_execute_directly{syscall_ptr: felt*, pedersen_ptr: HashBuiltin*, range_check_ptr}(
    contract_address: felt, calldata_len: felt, calldata: felt*
) {
    let execute_selector = 0x15d40a3d6ca2ac30f4031e42be28da9b056fef9bb7357ac5e85627ee876e5ad;
    call_contract(
        contract_address=contract_address,
        function_selector=execute_selector,
        calldata_size=calldata_len,
        calldata=calldata,
    );
    return ();
}

@external
@raw_output
func test_call_two_contracts{syscall_ptr: felt*}(
    contract_address_0: felt,
    function_selector_0: felt,
    calldata_0_len: felt,
    calldata_0: felt*,
    contract_address_1: felt,
    function_selector_1: felt,
    calldata_1_len: felt,
    calldata_1: felt*,
) -> (retdata_size: felt, retdata: felt*) {
    alloc_locals;

    let (retdata_0_len: felt, retdata_0: felt*) = call_contract(
        contract_address=contract_address_0,
        function_selector=function_selector_0,
        calldata_size=calldata_0_len,
        calldata=calldata_0,
    );
    let (retdata_1_len: felt, retdata_1: felt*) = call_contract(
        contract_address=contract_address_1,
        function_selector=function_selector_1,
        calldata_size=calldata_1_len,
        calldata=calldata_1,
    );

    let retdata: felt* = alloc();
    memcpy(dst=retdata, src=retdata_0, len=retdata_0_len);
    memcpy(dst=retdata + retdata_0_len, src=retdata_1, len=retdata_1_len);
    return (retdata_size=retdata_0_len + retdata_1_len, retdata=retdata);
}

@external
func test_call_contract_fail_with_attr_error_msg{syscall_ptr: felt*}(
    contract_address: felt, function_selector: felt
) -> () {
    with_attr error_message("Be aware of failure ahead...") {
        call_contract(
            contract_address=contract_address,
            function_selector=function_selector,
            calldata_size=0,
            calldata=new(),
        );
    }
    return ();
}

@external
func test_replace_class{syscall_ptr: felt*}(class_hash: felt) -> () {
    replace_class(class_hash=class_hash);
    return ();
}

@external
func test_deploy{syscall_ptr: felt*}(
    class_hash: felt,
    contract_address_salt: felt,
    constructor_calldata_len: felt,
    constructor_calldata: felt*,
    deploy_from_zero: felt,
) -> (contract_address: felt) {
    let (contract_address) = deploy(
        class_hash=class_hash,
        contract_address_salt=contract_address_salt,
        constructor_calldata_size=constructor_calldata_len,
        constructor_calldata=constructor_calldata,
        deploy_from_zero=deploy_from_zero,
    );
    return (contract_address=contract_address);
}

@external
func test_storage_var{syscall_ptr: felt*, pedersen_ptr: HashBuiltin*, range_check_ptr}() {
    number_map.write(key=1, value=39);
    let (val) = number_map.read(key=1);
    assert val = 39;
    return ();
}

@external
func test_contract_address{pedersen_ptr: HashBuiltin*, range_check_ptr}(
    salt: felt,
    class_hash: felt,
    constructor_calldata_len: felt,
    constructor_calldata: felt*,
    deployer_address: felt,
) -> (contract_address: felt) {
    let (contract_address) = get_contract_address{hash_ptr=pedersen_ptr}(
        salt=salt,
        class_hash=class_hash,
        constructor_calldata_size=constructor_calldata_len,
        constructor_calldata=constructor_calldata,
        deployer_address=deployer_address,
    );

    return (contract_address=contract_address);
}

@external
func foo() {
    return ();
}

@external
func invoke_call_chain{syscall_ptr: felt*}(calldata_len: felt, calldata: felt*) -> (res: felt) {
    // If the chain is too short, fail with division by zero.
    if ((calldata_len * (calldata_len - 1) * (calldata_len - 2)) == 0) {
        tempvar zero = calldata_len - calldata_len;
        return (res=1 / zero);
    }

    // Pop the parameters for the next call in the chain.
    let contract_id = calldata[0];
    let function_selector = calldata[1];
    let call_type = calldata[2];

    let calldata = &calldata[3];
    let calldata_len = calldata_len - 3;

    // Choose call type according to the following options:
    // 0 - call contract syscall. 1 - library call syscall. other - regular inner call.
    // The remaining items of the call_chain array are passed on as calldata.
    if (call_type == 0) {
        call_contract(
            contract_address=contract_id,
            function_selector=function_selector,
            calldata_size=calldata_len,
            calldata=calldata,
        );
        return (res=0);
    }
    if (call_type == 1) {
        library_call(
            class_hash=contract_id,
            function_selector=function_selector,
            calldata_size=calldata_len,
            calldata=calldata,
        );
        return (res=0);
    }

    let invoke_call_chain_selector = 0x0062c83572d28cb834a3de3c1e94977a4191469a4a8c26d1d7bc55305e640ed5;
    let fail_selector = 0x032564d7e0fe091d49b4c20f4632191e4ed6986bf993849879abfef9465def25;
    if (function_selector == invoke_call_chain_selector) {
        return invoke_call_chain(calldata_len=calldata_len, calldata=calldata);
    }
    if (function_selector == fail_selector) {
        fail();
        return (res=0);
    }

    return (res=0);
}

@external
func fail() {
    with_attr error_message("You shall not pass!") {
        assert 0 = 1;
    }
    return ();
}

@external
func recursive_fail(depth: felt) {
    if (depth == 0) {
        assert 0 = 1;
        return ();
    }
    recursive_fail(depth - 1);
    return ();
}

@external
func recurse(depth: felt) {
    if (depth == 0) {
        return ();
    }
    recurse(depth - 1);
    return ();
}

@external
func recursive_syscall{syscall_ptr: felt*}(
    contract_address: felt, function_selector: felt, depth: felt
) {
    alloc_locals;
    if (depth == 0) {
        return ();
    }
    local calldata: felt* = new(contract_address, function_selector, depth - 1);
    call_contract(
        contract_address=contract_address,
        function_selector=function_selector,
        calldata_size=3,
        calldata=calldata,
    );
    return ();
}

@external
func test_write_and_transfer{syscall_ptr: felt*}(
    key: felt, value: felt, to: felt, amount: felt, fee_token_address: felt
) -> (retdata_len: felt, retdata: felt*) {
    alloc_locals;
    storage_write(address=key, value=value);
    let caller = get_caller_address();
    local calldata: felt* = new(caller, to, amount, 0);
    let (retdata_len: felt, retdata: felt*) = call_contract(
        contract_address=fee_token_address,
        function_selector=TRANSFER_FROM_SELECTOR,
        calldata_size=4,
        calldata=calldata,
    );
    return (retdata_len=retdata_len, retdata=retdata);
}

@external
func test_get_block_number{syscall_ptr: felt*}(expected_block_number: felt) {
    let (block_number) = get_block_number();
    assert block_number = expected_block_number;
    return ();
}

@external
func test_get_block_timestamp{syscall_ptr: felt*}(expected_block_timestamp: felt) {
    let (block_timestamp) = get_block_timestamp();
    assert block_timestamp = expected_block_timestamp;
    return ();
}

@external
func test_get_sequencer_address{syscall_ptr: felt*}(expected_sequencer_address: felt) {
    let (sequencer_address) = get_sequencer_address();
    assert sequencer_address = expected_sequencer_address;
    return ();
}

@external
func test_get_tx_info{syscall_ptr: felt*, range_check_ptr}(
    expected_version: felt,
    expected_account_contract_address: felt,
    expected_max_fee: felt,
    expected_transaction_hash: felt,
    expected_chain_id: felt,
    expected_nonce: felt,
) {
    let (tx_info_ptr: TxInfo*) = get_tx_info();
    // Copy tx_info fields to make sure they were assigned a value during the system call.
    tempvar tx_info = [tx_info_ptr];

    assert tx_info.version = expected_version;
    assert tx_info.account_contract_address = expected_account_contract_address;
    assert tx_info.max_fee = expected_max_fee;
    assert tx_info.transaction_hash = expected_transaction_hash;
    assert tx_info.chain_id = expected_chain_id;
    assert tx_info.nonce = expected_nonce;
    assert tx_info.signature_len = 0;

    storage_write(address=300, value=tx_info.transaction_hash);
    storage_write(address=311, value=tx_info.chain_id);
    storage_write(address=322, value=tx_info.nonce);

    return ();
}

@external
func test_tx_version{syscall_ptr: felt*}(expected_version: felt) {
    let (tx_info: TxInfo*) = get_tx_info();
    assert tx_info.version = expected_version;

    return ();
}

@external
func test_count_actual_storage_changes{syscall_ptr: felt*, pedersen_ptr: HashBuiltin*}() {
    const address = 15;
    storage_write(address=address, value=0);
    storage_write(address=address, value=1);
    return ();
}

struct IndexAndValues {
    index: felt,
    values: (x: felt, y: felt),
}

@contract_interface
namespace MyContract {
    func xor_counters(index_and_x: IndexAndValues) {
    }
}

@storage_var
func two_counters(index: felt) -> (res: (felt, felt)) {
}

@storage_var
func ec_point() -> (res: EcPoint) {
}

// Advances the 'two_counters' storage variable by 'diff'.
@external
func advance_counter{syscall_ptr: felt*, pedersen_ptr: HashBuiltin*, range_check_ptr}(
    index: felt, diff_0: felt, diff_1: felt
) {
    let (val) = two_counters.read(index);
    two_counters.write(index, (val[0] + diff_0, val[1] + diff_1));
    return ();
}

@external
func xor_counters{
    syscall_ptr: felt*, pedersen_ptr: HashBuiltin*, range_check_ptr, bitwise_ptr: BitwiseBuiltin*
}(index_and_x: IndexAndValues) {
    let index = index_and_x.index;
    let x0 = index_and_x.values[0];
    let x1 = index_and_x.values[1];
    let (val) = two_counters.read(index);
    let (res0) = bitwise_xor(val[0], x0);
    let (res1) = bitwise_xor(val[1], x1);
    two_counters.write(index, (res0, res1));
    return ();
}

@external
func call_xor_counters{syscall_ptr: felt*, range_check_ptr}(
    address: felt, index_and_x: IndexAndValues
) {
    MyContract.xor_counters(address, index_and_x);
    return ();
}

@external
func test_ec_op{
    syscall_ptr: felt*, pedersen_ptr: HashBuiltin*, range_check_ptr, ec_op_ptr: EcOpBuiltin*
}() {
    let p = EcPoint(
        0x654fd7e67a123dd13868093b3b7777f1ffef596c2e324f25ceaf9146698482c,
        0x4fad269cbf860980e38768fe9cb6b0b9ab03ee3fe84cfde2eccce597c874fd8,
    );
    let q = EcPoint(
        0x3dbce56de34e1cfe252ead5a1f14fd261d520d343ff6b7652174e62976ef44d,
        0x4b5810004d9272776dec83ecc20c19353453b956e594188890b48467cb53c19,
    );
    let m = 0x6d232c016ef1b12aec4b7f88cc0b3ab662be3b7dd7adbce5209fcfdbd42a504;
    let (res) = ec_op(p=p, m=m, q=q);
    ec_point.write(res);
    return ();
}

@external
func add_signature_to_counters{pedersen_ptr: HashBuiltin*, range_check_ptr, syscall_ptr: felt*}(
    index: felt
) {
    let (signature_len: felt, signature: felt*) = get_tx_signature();
    assert signature_len = 2;
    let (val) = two_counters.read(index);
    two_counters.write(index, (val[0] + signature[0], val[1] + signature[1]));
    return ();
}

@external
func send_message{syscall_ptr: felt*}(to_address: felt) {
    alloc_locals;
    local payload: (felt, felt) = (12, 34);
    let (__fp__, _) = get_fp_and_pc();
    send_message_to_l1(to_address=to_address, payload_size=2, payload=cast(&payload, felt*));
    return ();
}

func emit_event_recurse{syscall_ptr: felt*, pedersen_ptr: HashBuiltin*, range_check_ptr}(
    counter: felt, keys_len: felt, keys: felt*, data_len: felt, data: felt*
) {
    if (counter == 0) {
        return ();
    }
    emit_event(keys_len, keys, data_len, data);
    return emit_event_recurse(counter - 1, keys_len, keys, data_len, data);
}

@external
func test_emit_events{syscall_ptr: felt*, pedersen_ptr: HashBuiltin*, range_check_ptr}(
    events_count: felt, keys_len: felt, keys: felt*, data_len: felt, data: felt*
) {
    return emit_event_recurse(events_count, keys_len, keys, data_len, data);
}
