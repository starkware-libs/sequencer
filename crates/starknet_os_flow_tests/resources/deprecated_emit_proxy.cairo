%lang starknet
%builtins pedersen range_check
from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.cairo_builtins import HashBuiltin
from starkware.starknet.common.syscalls import emit_event
@external
@raw_input
@raw_output
func __default__{syscall_ptr: felt*, pedersen_ptr: HashBuiltin*, range_check_ptr}(
    selector: felt, calldata_size: felt, calldata: felt*
) -> (retdata_size: felt, retdata: felt*) {
    emit_event(keys_len=calldata_size, keys=calldata, data_len=0, data=calldata);
    let (empty: felt*) = alloc();
    return (retdata_size=0, retdata=empty);
}
