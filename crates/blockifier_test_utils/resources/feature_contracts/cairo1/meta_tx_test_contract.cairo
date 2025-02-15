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

#[starknet::contract]
mod MetaTxTestContract {
    use starknet::storage::MutableVecTrait;
    use starknet::ContractAddress;
    use super::CallData;

    #[storage]
    struct Storage {
        call_data: starknet::storage::Vec::<CallData>,
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
            // TODO(lior, 01/03/2025): use push() once supported by the compiler.
            self.call_data.append().write(call_data);
        }
    }

    #[external(v0)]
    fn foo(ref self: ContractState, argument: felt252) {
        self.add_call_info(:argument);
    }

    #[external(v0)]
    fn execute_meta_tx_v0(
        ref self: ContractState,
        address: ContractAddress,
        entry_point_selector: felt252,
        calldata: Span<felt252>,
        _signature: Span<felt252>,
    ) {
        // TODO(lior, 01/03/2025): Replace `call_contract_syscall` with a meta transaction.
        starknet::syscalls::call_contract_syscall(:address, :entry_point_selector, :calldata)
            .unwrap();
        self.add_call_info(argument: 'NO_ARGUMENT');
    }
}
