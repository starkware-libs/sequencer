#[starknet::contract(account)]
mod Account {
    use array::ArrayTrait;
    use array::SpanTrait;
    use box::BoxTrait;
    use starknet::{
        ContractAddress,
        info::SyscallResultTrait,
        get_block_number,
        get_block_timestamp,
    };
    use zeroable::{IsZeroResult, NonZeroIntoImpl, Zeroable};

    // Round down the block number and timestamp when queried inside `__validate__`.
    const VALIDATE_BLOCK_NUMBER_ROUNDING: u64 = 100;
    const VALIDATE_TIMESTAMP_ROUNDING: u64 = 3600;

    #[storage]
    struct Storage {
    }

    #[constructor]
    fn constructor(ref self: ContractState, is_validate_: bool) {
        test_block_info(is_validate: is_validate_);
    }

    #[external(v0)]
    fn __validate_deploy__(
        self: @ContractState,
        class_hash: felt252,
        contract_address_salt: felt252,
        is_validate_: bool
    ) -> felt252 {
        test_block_info(is_validate: true)
    }

    #[external(v0)]
    fn __validate_declare__(self: @ContractState, class_hash: felt252) -> felt252 {
        test_block_info(is_validate: true)
    }

    #[external(v0)]
    fn __validate__(
        self: @ContractState,
        contract_address: ContractAddress,
        selector: felt252,
        calldata: Array<felt252>
    ) -> felt252 {
        test_block_info(is_validate: true)
    }

    #[external(v0)]
    #[raw_output]
    fn __execute__(
        self: @ContractState,
        contract_address: ContractAddress,
        selector: felt252,
        calldata: Array<felt252>
    ) -> Span<felt252> {
        array![test_block_info(is_validate: false)].span()
    }


    fn test_block_info(is_validate: bool) -> felt252 {
        let block_number = get_block_number();
        let block_timestamp = get_block_timestamp();
        test_given_block_info(
            block_number, block_timestamp, is_validate
        )
    }

    fn test_given_block_info (
        block_number: u64, block_timestamp: u64, is_validate: bool
    ) -> felt252 {
        // Verify the block number and timestamp.
        assert (VALIDATE_BLOCK_NUMBER_ROUNDING != 0, 'INVALID_ROUNDING');
        let (divided_block_number, _) = DivRem::div_rem(
            block_number, VALIDATE_BLOCK_NUMBER_ROUNDING.try_into().unwrap()
        );
        let block_number_for_validate = divided_block_number * VALIDATE_BLOCK_NUMBER_ROUNDING;
        let (divided_block_timestamp, _) = DivRem::div_rem(
            block_timestamp, VALIDATE_TIMESTAMP_ROUNDING.try_into().unwrap()
        );
        let block_timestamp_for_validate = (
            divided_block_timestamp * VALIDATE_TIMESTAMP_ROUNDING
        );

        if is_validate {
            assert (block_number == block_number_for_validate, 'INVALID_BLOCK_NUMBER');
            assert (block_timestamp == block_timestamp_for_validate, 'INVALID_BLOCK_TIMESTAMP');
            return starknet::VALIDATED;
        }
        assert (!is_validate, 'INVALID_IS_VALIDATE');
        starknet::VALIDATED
    }
}
