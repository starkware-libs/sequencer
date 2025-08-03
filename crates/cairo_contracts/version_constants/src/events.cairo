use starknet::ContractAddress;

#[derive(Debug, Drop, PartialEq, starknet::Event)]
pub struct FileCreated {
    pub file_id: felt252,
    pub size: usize,
    pub created_at_block: u64,
    pub last_modifier: ContractAddress,
}

#[derive(Debug, Drop, PartialEq, starknet::Event)]
pub struct FileModified {
    pub file_id: felt252,
    pub size: usize,
    pub modified_at_block: u64,
    pub last_modifier: ContractAddress,
}

#[derive(Debug, Drop, PartialEq, starknet::Event)]
pub struct FileDeleted {
    pub file_id: felt252,
}
