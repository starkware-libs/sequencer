use starknet::ContractAddress;

#[derive(Drop, starknet::Store)]
struct CallData {
    caller_address: ContractAddress,
    account_contract_address: ContractAddress,
    tx_version: felt252,
    argument: felt252,
    transaction_hash: felt252,
    signature: felt252,
    max_fee: u128,
    resource_bounds_len: usize,
    nonce: felt252,
}

#[starknet::contract(account)]
mod MetaTxTestContract {
    use starknet::ContractAddress;
    use starknet::SyscallResultTrait;
    use starknet::storage::MutableVecTrait;
    use super::CallData;

    #[storage]
    struct Storage {
        call_data: starknet::storage::Vec<CallData>,
    }

    #[generate_trait]
    impl InternalFunctions of InternalFunctionsTrait {
        fn add_call_info(ref self: ContractState, argument: felt252) {
            let execution_info = starknet::get_execution_info().unbox();
            let tx_info = execution_info.tx_info.unbox();

            let mut tx_signature = execution_info.tx_info.signature;
            let signature = *tx_signature.pop_front().unwrap_or(@'NO_SIGNATURE');

            let call_data = CallData {
                caller_address: starknet::get_caller_address(),
                account_contract_address: tx_info.account_contract_address,
                tx_version: tx_info.version,
                argument,
                transaction_hash: tx_info.transaction_hash,
                signature,
                max_fee: tx_info.max_fee,
                resource_bounds_len: tx_info.resource_bounds.len(),
                nonce: tx_info.nonce,
            };
            self.call_data.push(call_data);
        }
    }

    // `__validate__` must be implemented if `__execute__` is.
    #[external(v0)]
    fn __validate__(ref self: ContractState, argument: felt252) {}

    #[external(v0)]
    fn __execute__(ref self: ContractState, argument: felt252) {
        self.add_call_info(:argument);
    }

    extern fn meta_tx_v0_syscall(
        address: ContractAddress,
        entry_point_selector: felt252,
        calldata: Span<felt252>,
        signature: Span<felt252>,
    ) -> starknet::SyscallResult<Span<felt252>> implicits(GasBuiltin, System) nopanic;

    #[external(v0)]
    fn execute_meta_tx_v0(
        ref self: ContractState,
        address: ContractAddress,
        entry_point_selector: felt252,
        calldata: Span<felt252>,
        signature: Span<felt252>,
        should_revert: bool,
    ) {
        let res = meta_tx_v0_syscall(:address, :entry_point_selector, :calldata, :signature);
        if should_revert {
            let err = res.unwrap_err();
            assert!(err == array!['Invalid argument'], "Unexpected error: {:?}", err);
        } else {
            res.unwrap_syscall();
        }
        self.add_call_info(argument: 'NO_ARGUMENT');
    }
}
