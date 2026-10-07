extern fn coupon_buy<T>() -> T nopanic;

#[inline(never)]
fn add_one(x: felt252) -> felt252 nopanic {
    x
}

#[starknet::contract]
mod coupon_contract {
    use super::{add_one, coupon_buy};

    #[storage]
    struct Storage {}

    #[external(v0)]
    fn call_with_coupon(ref self: ContractState, x: felt252) -> felt252 {
        let coupon: add_one::Coupon = coupon_buy();
        add_one(x, __coupon__: coupon)
    }
}
