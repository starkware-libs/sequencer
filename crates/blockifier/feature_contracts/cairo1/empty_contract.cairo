#[starknet::contract]
mod TestContract {
    #[storage]
    struct Storage {}
}


#[external(v0)]
    fn empty_func(self: @ContractState) {
    }
