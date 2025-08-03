use snforge_std::{EventSpyAssertionsTrait, spy_events, start_cheat_block_number_global};
use starkware_utils_testing::constants as testing_constants;
use starkware_utils_testing::test_utils::cheat_caller_address_once;
use version_constants::interface::{
    FileWriteModeEnum, IVersionConstantsDispatcher, IVersionConstantsDispatcherTrait,
};
use version_constants::tests::test_utils::{
    BLOCK_NUMBER, FILE_ID, create_file, init_version_constants,
};

#[test]
fn test_create_file() {
    let contract_address = init_version_constants();
    let dispatcher = IVersionConstantsDispatcher { contract_address };
    let mut spy = spy_events();

    let file_id: felt252 = 'new_file_id';
    let data: ByteArray = "Hello, world!";
    let data_len = data.len();
    let mode = FileWriteModeEnum::New;

    start_cheat_block_number_global(BLOCK_NUMBER);
    cheat_caller_address_once(contract_address, testing_constants::DUMMY_ADDRESS);
    let written_bytes = dispatcher.write_file(file_id, data, mode);

    assert_eq!(written_bytes, data_len);

    spy
        .assert_emitted(
            @array![
                (
                    contract_address,
                    version_constants::version_constants::version_constants::Event::FileCreated(
                        version_constants::events::FileCreated {
                            file_id,
                            size: data_len,
                            created_at_block: BLOCK_NUMBER,
                            last_modifier: testing_constants::DUMMY_ADDRESS,
                        },
                    ),
                ),
            ],
        );
}

#[test]
#[should_panic(expected: 'File already exists')]
fn test_create_existing_file() {
    let contract_address = init_version_constants();
    let dispatcher = IVersionConstantsDispatcher { contract_address };

    create_file(dispatcher);

    let data: ByteArray = "Hello, Cairo!";

    let mode = FileWriteModeEnum::New;
    dispatcher.write_file(FILE_ID, data, mode);
}

#[test]
fn test_get_metadata() {
    let contract_address = init_version_constants();
    let dispatcher = IVersionConstantsDispatcher { contract_address };

    let data_len = create_file(dispatcher);

    let metadata = dispatcher.get_file_metadata(FILE_ID);

    assert_eq!(metadata.size, data_len);
    assert_eq!(metadata.created_at_block, BLOCK_NUMBER);
    assert_eq!(metadata.modified_at_block, BLOCK_NUMBER);
    assert_eq!(metadata.last_modifier, testing_constants::DUMMY_ADDRESS);
}

#[test]
fn test_overwrite_file() {
    let contract_address = init_version_constants();
    let dispatcher = IVersionConstantsDispatcher { contract_address };

    create_file(dispatcher);

    let mode = FileWriteModeEnum::Overwrite(0);
    let data: ByteArray = "Hello, Cairo!";
    let data_len = data.len();

    let mut spy = spy_events();
    start_cheat_block_number_global(BLOCK_NUMBER + 1);
    cheat_caller_address_once(contract_address, testing_constants::OPERATOR);
    let written_bytes = dispatcher.write_file(FILE_ID, data, mode);

    assert_eq!(written_bytes, data_len);

    spy
        .assert_emitted(
            @array![
                (
                    contract_address,
                    version_constants::version_constants::version_constants::Event::FileModified(
                        version_constants::events::FileModified {
                            file_id: FILE_ID,
                            size: data_len,
                            modified_at_block: BLOCK_NUMBER + 1,
                            last_modifier: testing_constants::OPERATOR,
                        },
                    ),
                ),
            ],
        );
}

#[test]
#[should_panic(expected: 'File not found')]
fn test_overwrite_non_existent_file() {
    let contract_address = init_version_constants();
    let dispatcher = IVersionConstantsDispatcher { contract_address };

    let mode = FileWriteModeEnum::Overwrite(0);
    let data: ByteArray = "Hello, Cairo!";

    dispatcher.write_file(FILE_ID, data, mode);
}

#[test]
fn test_append_file() {
    let contract_address = init_version_constants();
    let dispatcher = IVersionConstantsDispatcher { contract_address };

    let data_len = create_file(dispatcher);

    let mode = FileWriteModeEnum::Append;
    let data: ByteArray = "Hello, Cairo!";
    let append_data_len = data.len();

    let mut spy = spy_events();
    start_cheat_block_number_global(BLOCK_NUMBER + 1);
    cheat_caller_address_once(contract_address, testing_constants::OPERATOR);
    let written_bytes = dispatcher.write_file(FILE_ID, data, mode);

    assert_eq!(written_bytes, append_data_len);

    spy
        .assert_emitted(
            @array![
                (
                    contract_address,
                    version_constants::version_constants::version_constants::Event::FileModified(
                        version_constants::events::FileModified {
                            file_id: FILE_ID,
                            size: data_len + append_data_len,
                            modified_at_block: BLOCK_NUMBER + 1,
                            last_modifier: testing_constants::OPERATOR,
                        },
                    ),
                ),
            ],
        );
}

#[test]
#[should_panic(expected: 'File not found')]
fn test_append_non_existent_file() {
    let contract_address = init_version_constants();
    let dispatcher = IVersionConstantsDispatcher { contract_address };

    let mode = FileWriteModeEnum::Append;
    let data: ByteArray = "Hello, Cairo!";

    dispatcher.write_file(FILE_ID, data, mode);
}

#[test]
fn test_get_metadata_after_overwrite() {
    let contract_address = init_version_constants();
    let dispatcher = IVersionConstantsDispatcher { contract_address };

    create_file(dispatcher);
    let mode = FileWriteModeEnum::Overwrite(0);
    let data: ByteArray = "Hello, Cairo!!!!!!";
    let modify_data_len = data.len();

    start_cheat_block_number_global(BLOCK_NUMBER + 1);
    cheat_caller_address_once(contract_address, testing_constants::OPERATOR);
    let written_bytes = dispatcher.write_file(FILE_ID, data, mode);

    assert_eq!(written_bytes, modify_data_len);

    let metadata = dispatcher.get_file_metadata(FILE_ID);

    assert_eq!(metadata.size, modify_data_len);
    assert_eq!(metadata.created_at_block, BLOCK_NUMBER);
    assert_eq!(metadata.modified_at_block, BLOCK_NUMBER + 1);
    assert_eq!(metadata.last_modifier, testing_constants::OPERATOR);
}

#[test]
fn test_get_metadata_after_append() {
    let contract_address = init_version_constants();
    let dispatcher = IVersionConstantsDispatcher { contract_address };

    let data_len = create_file(dispatcher);

    let mode = FileWriteModeEnum::Append;
    let data: ByteArray = "Hello, Cairo!";
    let append_data_len = data.len();

    start_cheat_block_number_global(BLOCK_NUMBER + 1);
    cheat_caller_address_once(contract_address, testing_constants::OPERATOR);
    let written_bytes = dispatcher.write_file(FILE_ID, data, mode);

    assert_eq!(written_bytes, append_data_len);

    let metadata = dispatcher.get_file_metadata(FILE_ID);

    assert_eq!(metadata.size, data_len + append_data_len);
    assert_eq!(metadata.created_at_block, BLOCK_NUMBER);
    assert_eq!(metadata.modified_at_block, BLOCK_NUMBER + 1);
    assert_eq!(metadata.last_modifier, testing_constants::OPERATOR);
}

#[test]
#[should_panic(expected: 'File not found')]
fn test_get_metadata_of_non_existent_file() {
    let contract_address = init_version_constants();
    let dispatcher = IVersionConstantsDispatcher { contract_address };

    dispatcher.get_file_metadata(FILE_ID);
}

#[test]
fn test_read_file() {
    let contract_address = init_version_constants();
    let dispatcher = IVersionConstantsDispatcher { contract_address };

    let data_len = create_file(dispatcher);

    let data = dispatcher.read_file(FILE_ID, 0, data_len);

    assert_eq!(data, "Hello, world!");
}

#[test]
fn test_read_file_with_exceeding_length() {
    let contract_address = init_version_constants();
    let dispatcher = IVersionConstantsDispatcher { contract_address };

    let data_len = create_file(dispatcher);

    let data = dispatcher.read_file(FILE_ID, 0, data_len + 1);

    assert_eq!(data, "Hello, world!");
}

#[test]
#[should_panic(expected: 'Offset out of bounds')]
fn test_read_file_with_offset_out_of_bounds() {
    let contract_address = init_version_constants();
    let dispatcher = IVersionConstantsDispatcher { contract_address };

    let data_len = create_file(dispatcher);

    dispatcher.read_file(FILE_ID, data_len + 1, data_len);
}

#[test]
fn test_read_file_with_offset_and_length() {
    let contract_address = init_version_constants();
    let dispatcher = IVersionConstantsDispatcher { contract_address };

    create_file(dispatcher);

    let data = dispatcher.read_file(FILE_ID, 7, 5); // "world"

    assert_eq!(data, "world");
}

#[test]
fn test_write_with_offset() {
    let contract_address = init_version_constants();
    let dispatcher = IVersionConstantsDispatcher { contract_address };

    let data_len = create_file(dispatcher);

    let data: ByteArray = "Cairo";
    let write_data_len = data.len();

    let mode = FileWriteModeEnum::Overwrite(7);
    let written_bytes = dispatcher
        .write_file(FILE_ID, data, mode); // "Hello, world!" -> "Hello, Cairo!"

    assert_eq!(written_bytes, write_data_len);

    let data = dispatcher.read_file(FILE_ID, 0, data_len);

    assert_eq!(data, "Hello, Cairo!");
}

#[test]
fn test_write_with_offset_and_exceeding_length() {
    let contract_address = init_version_constants();
    let dispatcher = IVersionConstantsDispatcher { contract_address };

    create_file(dispatcher);

    let data: ByteArray = "Cairo!!!!!";
    let write_data_len = data.len();

    let mode = FileWriteModeEnum::Overwrite(7);
    start_cheat_block_number_global(BLOCK_NUMBER + 1);
    cheat_caller_address_once(contract_address, testing_constants::OPERATOR);
    let written_bytes = dispatcher
        .write_file(FILE_ID, data, mode); // "Hello, Cairo!" -> "Hello, Cairo!!!!!"

    assert_eq!(written_bytes, write_data_len);

    let file_data: ByteArray = "Hello, Cairo!!!!!";

    let file_size: usize = file_data.len();

    let data = dispatcher.read_file(FILE_ID, 0, file_size);

    assert_eq!(data, "Hello, Cairo!!!!!");

    let metadata = dispatcher.get_file_metadata(FILE_ID);

    assert_eq!(metadata.size, file_size);
    assert_eq!(metadata.created_at_block, BLOCK_NUMBER);
    assert_eq!(metadata.modified_at_block, BLOCK_NUMBER + 1);
    assert_eq!(metadata.last_modifier, testing_constants::OPERATOR);
}

#[test]
fn test_write_empty_file() {
    let contract_address = init_version_constants();
    let dispatcher = IVersionConstantsDispatcher { contract_address };

    let data: ByteArray = "";
    let write_data_len = data.len();

    let mode = FileWriteModeEnum::New;
    let written_bytes = dispatcher.write_file(FILE_ID, data, mode);

    assert_eq!(written_bytes, write_data_len);

    let data = dispatcher.read_file(FILE_ID, 0, 1);

    assert_eq!(data, "");
}

#[test]
fn test_overwrite_empty_string() {
    let contract_address = init_version_constants();
    let dispatcher = IVersionConstantsDispatcher { contract_address };

    let data_len = create_file(dispatcher);

    let data: ByteArray = "";
    let write_data_len = data.len();

    let mode = FileWriteModeEnum::Overwrite(2);
    let written_bytes = dispatcher
        .write_file(FILE_ID, data, mode); // "Hello, world!" -> "Hello, world!"

    assert_eq!(written_bytes, write_data_len);

    let data = dispatcher.read_file(FILE_ID, 0, data_len);

    assert_eq!(data, "Hello, world!");
}

#[test]
fn test_append_empty_string() {
    let contract_address = init_version_constants();
    let dispatcher = IVersionConstantsDispatcher { contract_address };

    let data_len = create_file(dispatcher);

    let data: ByteArray = "";
    let write_data_len = data.len();

    let mode = FileWriteModeEnum::Append;
    let written_bytes = dispatcher
        .write_file(FILE_ID, data, mode); // "Hello, world!" -> "Hello, world!"

    assert_eq!(written_bytes, write_data_len);

    let data = dispatcher.read_file(FILE_ID, 0, data_len);

    assert_eq!(data, "Hello, world!");
}

#[test]
fn test_read_zero_length() {
    let contract_address = init_version_constants();
    let dispatcher = IVersionConstantsDispatcher { contract_address };

    create_file(dispatcher);

    let data = dispatcher.read_file(FILE_ID, 0, 0);

    assert_eq!(data, "");
}

#[test]
#[should_panic(expected: 'File not found')]
fn test_delete_file() {
    let contract_address = init_version_constants();
    let dispatcher = IVersionConstantsDispatcher { contract_address };

    create_file(dispatcher);

    let mut spy = spy_events();
    start_cheat_block_number_global(BLOCK_NUMBER + 1);
    cheat_caller_address_once(contract_address, testing_constants::OPERATOR);
    dispatcher.delete_file(FILE_ID);

    spy
        .assert_emitted(
            @array![
                (
                    contract_address,
                    version_constants::version_constants::version_constants::Event::FileDeleted(
                        version_constants::events::FileDeleted { file_id: FILE_ID },
                    ),
                ),
            ],
        );

    let data = dispatcher.read_file(FILE_ID, 0, 1);
    assert_eq!(data, "");
    dispatcher.get_file_metadata(FILE_ID);
}

#[test]
fn test_create_delete_create_file() {
    let contract_address = init_version_constants();
    let dispatcher = IVersionConstantsDispatcher { contract_address };

    create_file(dispatcher);

    dispatcher.delete_file(FILE_ID);

    let data: ByteArray = "Hello";
    let data_len = data.len();
    let mode = FileWriteModeEnum::New;

    start_cheat_block_number_global(BLOCK_NUMBER + 1);
    cheat_caller_address_once(contract_address, testing_constants::OPERATOR);
    let written_bytes = dispatcher.write_file(FILE_ID, data, mode);

    assert_eq!(written_bytes, data_len);

    let metadata = dispatcher.get_file_metadata(FILE_ID);

    assert_eq!(metadata.size, data_len);
    assert_eq!(metadata.created_at_block, BLOCK_NUMBER + 1);
    assert_eq!(metadata.modified_at_block, BLOCK_NUMBER + 1);
    assert_eq!(metadata.last_modifier, testing_constants::OPERATOR);
}

#[test]
#[should_panic(expected: 'File not found')]
fn test_create_delete_overwrite_file() {
    let contract_address = init_version_constants();
    let dispatcher = IVersionConstantsDispatcher { contract_address };

    create_file(dispatcher);

    dispatcher.delete_file(FILE_ID);

    let data: ByteArray = "Hello, Cairo!";
    let mode = FileWriteModeEnum::Overwrite(0);
    dispatcher.write_file(FILE_ID, data, mode);
}

#[test]
#[should_panic(expected: 'File not found')]
fn test_create_delete_append_file() {
    let contract_address = init_version_constants();
    let dispatcher = IVersionConstantsDispatcher { contract_address };

    create_file(dispatcher);

    dispatcher.delete_file(FILE_ID);

    let data: ByteArray = "Hello, Cairo!";
    let mode = FileWriteModeEnum::Append;
    dispatcher.write_file(FILE_ID, data, mode);
}

#[test]
#[should_panic(expected: 'File not found')]
fn test_create_delete_delete_file() {
    let contract_address = init_version_constants();
    let dispatcher = IVersionConstantsDispatcher { contract_address };

    create_file(dispatcher);

    dispatcher.delete_file(FILE_ID);

    dispatcher.delete_file(FILE_ID);
}

#[test]
fn test_create_two_files() {
    let contract_address = init_version_constants();
    let dispatcher = IVersionConstantsDispatcher { contract_address };

    create_file(dispatcher);

    let data: ByteArray = "Hello, Cairo!";
    let data_len = data.len();
    let mode = FileWriteModeEnum::New;

    start_cheat_block_number_global(BLOCK_NUMBER + 1);
    cheat_caller_address_once(contract_address, testing_constants::OPERATOR);
    let written_bytes = dispatcher.write_file('new_file_id', data, mode);

    assert_eq!(written_bytes, data_len);

    let metadata = dispatcher.get_file_metadata(FILE_ID);

    assert_eq!(metadata.size, data_len);
    assert_eq!(metadata.created_at_block, BLOCK_NUMBER);
    assert_eq!(metadata.modified_at_block, BLOCK_NUMBER);
    assert_eq!(metadata.last_modifier, testing_constants::DUMMY_ADDRESS);

    let metadata = dispatcher.get_file_metadata('new_file_id');

    assert_eq!(metadata.size, data_len);
    assert_eq!(metadata.created_at_block, BLOCK_NUMBER + 1);
    assert_eq!(metadata.modified_at_block, BLOCK_NUMBER + 1);
    assert_eq!(metadata.last_modifier, testing_constants::OPERATOR);
}

#[test]
fn test_two_writes_and_read() {
    let contract_address = init_version_constants();
    let dispatcher = IVersionConstantsDispatcher { contract_address };

    let data: ByteArray = "Hello";
    let first_data_len = data.len();
    let mode = FileWriteModeEnum::New;

    dispatcher.write_file(FILE_ID, data, mode);

    let data: ByteArray = ", world!";
    let second_data_len = data.len();
    let mode = FileWriteModeEnum::Append;

    dispatcher.write_file(FILE_ID, data, mode);

    let data = dispatcher.read_file(FILE_ID, 0, first_data_len + second_data_len);

    assert_eq!(data, "Hello, world!");
}
// #[test]
// fn test_big_file() {
//     let contract_address = init_version_constants();
//     let dispatcher = IVersionConstantsDispatcher { contract_address };

//     let mut data: ByteArray = "";
//     let word: felt252 = 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'; // felt size is 31 bytes

//     for _ in 0_u32..5_000_u32 {
//         data.append_word(word, 31);
//     }
//     //let write_data = @data;
//     let data_len = data.len();
//     let mode = FileWriteModeEnum::New;

//     let written_bytes = dispatcher.write_file(FILE_ID, data, mode);

//     println!("written_bytes: {}", written_bytes);
//     assert_eq!(written_bytes, data_len);
//     //let read_data = dispatcher.read_file(FILE_ID, 0, data_len);

//     //assert_eq!(write_data, @read_data);
// }


