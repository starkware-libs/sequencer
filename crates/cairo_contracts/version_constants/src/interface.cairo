use starknet::ContractAddress;
#[starknet::interface]
pub trait IVersionConstants<TContractState> {
    /// Returns the metadata of the file
    fn get_file_metadata(self: @TContractState, file_id: felt252) -> FileMetadata;

    /// Reads length bytes (or until the end of the file, whichever comes first) of the file from
    /// offset
    fn read_file(
        self: @TContractState, file_id: felt252, offset: usize, length: usize,
    ) -> ByteArray;

    /// Writes to the file with mode
    /// Returns the number of bytes written
    fn write_file(
        ref self: TContractState, file_id: felt252, data: ByteArray, mode: FileWriteModeEnum,
    ) -> usize;

    /// Deletes the file by file_id
    fn delete_file(ref self: TContractState, file_id: felt252);
}

#[derive(Debug, PartialEq, Drop, Serde, Copy, starknet::Store)]
pub struct FileMetadata {
    /// File size in bytes
    pub size: usize,
    /// Block number when the file was created
    pub created_at_block: u64,
    /// Block number of the last modification
    pub modified_at_block: u64,
    /// Address that last modified the file
    pub last_modifier: ContractAddress,
}

#[derive(Debug, PartialEq, Drop, Serde, Copy)]
pub enum FileWriteModeEnum {
    New,
    Overwrite: usize, // offset to overwrite from
    Append,
}
