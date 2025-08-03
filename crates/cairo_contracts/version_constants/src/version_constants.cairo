#[starknet::contract]
pub mod version_constants {
    use core::panic_with_felt252;
    use starknet::storage::{Map, StorageMapReadAccess, StorageMapWriteAccess};
    use starknet::{get_block_number, get_caller_address};
    use version_constants::interface::{FileMetadata, FileWriteModeEnum, IVersionConstants};
    use version_constants::{errors, events};

    #[storage]
    struct Storage {
        pub metadata: Map<felt252, Option<FileMetadata>>,
        pub storage: Map<felt252, ByteArray>,
    }

    #[event]
    #[derive(Drop, starknet::Event)]
    pub enum Event {
        FileCreated: events::FileCreated,
        FileModified: events::FileModified,
        FileDeleted: events::FileDeleted,
    }

    #[constructor]
    pub fn constructor(ref self: ContractState) {}

    #[abi(embed_v0)]
    impl VersionConstantsImpl of IVersionConstants<ContractState> {
        fn get_file_metadata(self: @ContractState, file_id: felt252) -> FileMetadata {
            match self.metadata.read(file_id) {
                Some(metadata) => metadata,
                None => panic_with_felt252(errors::FILE_NOT_FOUND),
            }
        }

        fn read_file(
            self: @ContractState, file_id: felt252, offset: usize, length: usize,
        ) -> ByteArray {
            let data = self.storage.read(file_id);
            substring(data, offset, length)
        }

        fn write_file(
            ref self: ContractState, file_id: felt252, data: ByteArray, mode: FileWriteModeEnum,
        ) -> usize {
            let block_number = get_block_number();
            let caller_address = get_caller_address();
            let data_len = data.len();
            match mode {
                FileWriteModeEnum::New => {
                    if self.metadata.read(file_id).is_some() {
                        panic_with_felt252(errors::FILE_ALREADY_EXISTS);
                    }
                    self.storage.write(file_id, data);
                    self
                        .metadata
                        .write(
                            file_id,
                            Option::Some(
                                FileMetadata {
                                    size: data_len,
                                    created_at_block: block_number,
                                    modified_at_block: block_number,
                                    last_modifier: caller_address,
                                },
                            ),
                        );
                    self
                        .emit(
                            events::FileCreated {
                                file_id,
                                size: data_len,
                                created_at_block: block_number,
                                last_modifier: caller_address,
                            },
                        );
                    data_len
                },
                FileWriteModeEnum::Overwrite(offset) => {
                    let wrapped_metadata = self.metadata.read(file_id);
                    if wrapped_metadata.is_none() {
                        panic_with_felt252(errors::FILE_NOT_FOUND);
                    }
                    let mut metadata = wrapped_metadata.unwrap();
                    metadata.modified_at_block = block_number;
                    metadata.last_modifier = caller_address;

                    let existing_data = self.storage.read(file_id);
                    let new_data = replace_range(@existing_data, @data, offset);
                    let new_len = new_data.len();
                    metadata.size = new_len;

                    self.storage.write(file_id, new_data);
                    self.metadata.write(file_id, Option::Some(metadata));
                    self
                        .emit(
                            events::FileModified {
                                file_id,
                                size: new_len,
                                modified_at_block: block_number,
                                last_modifier: caller_address,
                            },
                        );
                    data_len
                },
                FileWriteModeEnum::Append => {
                    let wrapped_metadata = self.metadata.read(file_id);
                    if wrapped_metadata.is_none() {
                        panic_with_felt252(errors::FILE_NOT_FOUND);
                    }
                    let mut metadata = wrapped_metadata.unwrap();
                    metadata.modified_at_block = block_number;
                    metadata.last_modifier = caller_address;

                    let existing_data = self.storage.read(file_id);
                    let append_offset = existing_data.len();
                    let new_data = replace_range(@existing_data, @data, append_offset);
                    let new_len = new_data.len();
                    metadata.size = new_len;

                    self.storage.write(file_id, new_data);
                    self.metadata.write(file_id, Option::Some(metadata));
                    self
                        .emit(
                            events::FileModified {
                                file_id,
                                size: new_len,
                                modified_at_block: block_number,
                                last_modifier: caller_address,
                            },
                        );
                    data_len
                },
            }
        }

        fn delete_file(ref self: ContractState, file_id: felt252) {
            if self.metadata.read(file_id).is_none() {
                panic_with_felt252(errors::FILE_NOT_FOUND);
            }
            self.storage.write(file_id, "");
            self.metadata.write(file_id, Option::None);
            self.emit(events::FileDeleted { file_id });
        }
    }

    /// Replaces bytes in the target ByteArray starting from offset with bytes from source ByteArray
    /// Returns a new ByteArray with the replaced content
    fn replace_range(target: @ByteArray, source: @ByteArray, offset: usize) -> ByteArray {
        let target_len = target.len();
        let source_len = source.len();

        let mut result: ByteArray = "";

        let mut i = 0;
        while i < offset && i < target_len {
            if let Option::Some(byte) = target.at(i) {
                result.append_byte(byte);
            }
            i += 1;
        }

        let mut j = 0;
        while j < source_len {
            if let Option::Some(byte) = source.at(j) {
                result.append_byte(byte);
            }
            j += 1;
        }

        let mut k = offset + source_len;
        while k < target_len {
            if let Option::Some(byte) = target.at(k) {
                result.append_byte(byte);
            }
            k += 1;
        }

        result
    }

    /// Extracts a substring from a ByteArray starting at offset with specified length
    /// Returns a new ByteArray containing the extracted bytes
    fn substring(data: ByteArray, offset: usize, length: usize) -> ByteArray {
        let data_len = data.len();

        if length == 0 || data_len == 0 {
            return "";
        }

        if offset >= data_len {
            panic_with_felt252(errors::OFFSET_OUT_OF_BOUNDS);
        }

        let actual_length = if offset + length > data_len {
            data_len - offset
        } else {
            length
        };

        let mut result: ByteArray = "";
        let mut current_pos = 0;

        for byte in data {
            if current_pos >= offset + actual_length {
                break;
            }
            if current_pos >= offset {
                result.append_byte(byte);
            }
            current_pos += 1;
        }

        result
    }
}
