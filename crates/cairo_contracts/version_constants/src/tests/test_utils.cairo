use openzeppelin_testing::deployment::declare_and_deploy;
use snforge_std::start_cheat_block_number_global;
use starknet::ContractAddress;
use starkware_utils_testing::constants as testing_constants;
use starkware_utils_testing::test_utils::cheat_caller_address_once;
use version_constants::interface::{
    FileWriteModeEnum, IVersionConstantsDispatcher, IVersionConstantsDispatcherTrait,
};

pub const BLOCK_NUMBER: u64 = 1000;
pub const FILE_ID: felt252 = 'file_id';

//pub const MAX_WRITE_SIZE: u32 = 19_000; // ~589KB, 19k times 31 bytes
//pub const MAX_READ_SIZE: u32 = 500; // ~15.5KB, 500 times 31 bytes

pub fn init_version_constants() -> ContractAddress {
    let calldata = array![];
    declare_and_deploy("version_constants", calldata)
}

pub fn create_file(dispatcher: IVersionConstantsDispatcher) -> usize {
    let data: ByteArray = "Hello, world!";
    let mode = FileWriteModeEnum::New;

    start_cheat_block_number_global(BLOCK_NUMBER);
    cheat_caller_address_once(dispatcher.contract_address, testing_constants::DUMMY_ADDRESS);
    dispatcher.write_file(FILE_ID, data, mode)
}
