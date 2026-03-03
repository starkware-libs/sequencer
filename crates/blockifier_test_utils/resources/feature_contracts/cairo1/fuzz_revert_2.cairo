// This contract is auto-generated. To regenerate, run:
// `UPDATE_EXPECT=1 cargo test -p blockifier_test_utils test_fuzz_revert_2_almost_identical`
#[starknet::contract]
mod FuzzRevertContract {
    #[storage]
    struct Storage {}

    /// This function is here to make this contract's class hash different from the main fuzz
    /// revert contract.
    #[external(v0)]
    fn dummy_function(ref self: ContractState) -> felt252 {
        return 100;
    }
}
