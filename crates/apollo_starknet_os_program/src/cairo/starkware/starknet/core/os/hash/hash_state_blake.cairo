from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.cairo_blake2s.blake2s import encode_felt252_data_and_calc_blake_hash
from starkware.cairo.common.memcpy import memcpy

// Stores a sequence of elements. New elements can be added to the hash state using
// hash_update() and hash_update_single().
// The final hash of the entire sequence can be obtained using hash_finalize().
struct HashState {
    start: felt*,
    end: felt*,
}

// Initializes a new HashState with no elements and returns it.
func hash_init() -> HashState {
    let (start: felt*) = alloc();
    return (HashState(start=start, end=start));
}

// Adds a single item to the HashState.
func hash_update_single{hash_state: HashState}(item: felt) {
    let current_end = hash_state.end;
    assert [current_end] = item;
    let hash_state = HashState(start=hash_state.start, end=current_end + 1);
    return ();
}

func hash_update_with_nested_hash{hash_state: HashState, range_check_ptr: felt}(
    data_ptr: felt*, data_length: felt
) {
    let (hash) = encode_felt252_data_and_calc_blake_hash(data_len=data_length, data=data_ptr);
    hash_update_single(item=hash);
    return ();
}

func hash_finalize{range_check_ptr: felt}(hash_state: HashState) -> felt {
    // %{print steps%}
    let data_length = hash_state.end - hash_state.start;
    // %{print steps%}
    let (hash) = encode_felt252_data_and_calc_blake_hash(
        data_len=data_length, data=hash_state.start
    );
    // %{print steps%}
    return (hash);
}
