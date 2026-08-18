// A minimal account whose constructor forwards the (logically empty)
// `account_deployment_data` span from the current transaction info into syscalls.
//
// DeployAccount V3 has no account-deployment data, so the OS builds that span with felt-zero
// endpoints. Re-emitting the span forces the syscall decoder to dereference those endpoints:
// native Blockifier (relocatable endpoints) succeeds, while an OS that uses `cast(0, felt*)`
// aborts with "Expected relocatable" (STARKNET-96). Used as a regression fixture.
#[starknet::contract(account)]
mod Account {
    use core::array::ArrayTrait;
    use core::box::BoxTrait;
    use starknet::syscalls::{call_contract_syscall, emit_event_syscall};
    use starknet::{ContractAddress, SyscallResultTrait};

    #[storage]
    struct Storage {
        cairo0_proxy_address: ContractAddress,
    }

    #[constructor]
    fn constructor(ref self: ContractState, cairo0_proxy_address: felt252) {
        let cairo0_proxy_address: ContractAddress = cairo0_proxy_address.try_into().unwrap();
        self.cairo0_proxy_address.write(cairo0_proxy_address);

        let execution_info = starknet::get_execution_info().unbox();
        let tx_info = execution_info.tx_info.unbox();
        let mut event_data: Array<felt252> = ArrayTrait::new();
        event_data.append(1);
        emit_event_syscall(tx_info.account_deployment_data, event_data.span()).unwrap_syscall();
    }

    #[external(v0)]
    fn __validate_deploy__(
        self: @ContractState,
        class_hash: felt252,
        contract_address_salt: felt252,
        cairo0_proxy_address: felt252,
    ) -> felt252 {
        starknet::VALIDATED
    }

    #[external(v0)]
    fn __validate_declare__(self: @ContractState, class_hash: felt252) -> felt252 {
        starknet::VALIDATED
    }

    #[external(v0)]
    fn __validate__(
        self: @ContractState,
        contract_address: ContractAddress,
        selector: felt252,
        calldata: Array<felt252>,
    ) -> felt252 {
        starknet::VALIDATED
    }

    #[external(v0)]
    #[raw_output]
    fn __execute__(
        ref self: ContractState,
        contract_address: ContractAddress,
        selector: felt252,
        calldata: Array<felt252>,
    ) -> Span<felt252> {
        let cairo0_proxy_address = self.cairo0_proxy_address.read();
        let execution_info = starknet::get_execution_info().unbox();
        let tx_info = execution_info.tx_info.unbox();
        call_contract_syscall(
            cairo0_proxy_address,
            selector!("read_write_read"),
            tx_info.account_deployment_data,
        )
            .unwrap_syscall();

        let empty: Array<felt252> = ArrayTrait::new();
        empty.span()
    }
}
