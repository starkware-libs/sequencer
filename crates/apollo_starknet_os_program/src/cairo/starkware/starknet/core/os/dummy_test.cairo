from starkware.cairo.common.cairo_builtins import HashBuiltin

func dummy{pedersen_ptr: HashBuiltin*}(offset_increase: felt) {
    assert pedersen_ptr.x = 1;
    assert pedersen_ptr.y = 2;
    let pedersen_ptr = pedersen_ptr + offset_increase;
    return ();
}
