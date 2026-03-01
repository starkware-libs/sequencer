use cairo_vm::program_hash::{compute_program_hash_chain, ProgramHashError as VmProgramHashError};
use cairo_vm::types::errors::program_errors::ProgramError;
use cairo_vm::types::program::Program;
use serde::{Deserialize, Serialize};
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::{Blake2Felt252, Pedersen, StarkHash};

use crate::{AGGREGATOR_PROGRAM, OS_PROGRAM, VIRTUAL_OS_PROGRAM};

#[cfg(test)]
#[path = "program_hash_test.rs"]
pub mod test;

#[derive(thiserror::Error, Debug)]
pub enum ProgramHashError {
    #[error(transparent)]
    Program(#[from] ProgramError),
    #[error(transparent)]
    VmProgramHash(#[from] VmProgramHashError),
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct ProgramHashes {
    pub os: Felt,
    pub virtual_os: Felt,
    pub aggregator: Felt,
    pub aggregator_with_prefix: Felt,
}

pub struct AggregatorHash {
    pub with_prefix: Felt,
    pub without_prefix: Felt,
}

const BOOTLOADER_VERSION: usize = 0;

fn pad_to_32_bytes(data: &[u8]) -> [u8; 32] {
    let mut padded = [0; 32];
    let len = data.len();
    if len > 32 {
        panic!("Data length exceeds 32 bytes.");
    }
    padded[32 - len..].copy_from_slice(data);
    padded
}

fn compute_program_hash(program: &Program) -> Result<Felt, ProgramHashError> {
    Ok(compute_program_hash_chain(&program.get_stripped_program()?, BOOTLOADER_VERSION)?)
}

/// Computes the program hash using Blake2s.
/// Hashes the full program header (bootloader_version, main, n_builtins, builtins) followed by
/// program data, matching the bootloader's Cairo implementation.
// TODO(Avi): Move this function to cairo_vm and share logic with compute_program_hash_chain.
pub fn compute_program_hash_blake(program: &Program) -> Result<Felt, ProgramHashError> {
    let stripped_program = program.get_stripped_program()?;

    let builtin_list: Vec<Felt> = stripped_program
        .builtins
        .iter()
        .map(|b| Felt::from_bytes_be_slice(b.to_str().as_bytes()))
        .collect();

    let program_header = vec![
        Felt::from(BOOTLOADER_VERSION),
        Felt::from(stripped_program.main),
        Felt::from(stripped_program.builtins.len()),
    ];

    let program_data: Vec<Felt> = stripped_program
        .data
        .iter()
        .map(|entry| entry.get_int_ref().copied().expect("Program data must contain felts."))
        .collect();

    let data_chain: Vec<Felt> =
        program_header.into_iter().chain(builtin_list).chain(program_data).collect();

    Ok(Blake2Felt252::encode_felt252_data_and_calc_blake_hash(&data_chain))
}

pub fn compute_os_program_hash() -> Result<Felt, ProgramHashError> {
    compute_program_hash(&OS_PROGRAM)
}

pub fn compute_virtual_os_program_hash() -> Result<Felt, ProgramHashError> {
    compute_program_hash_blake(&VIRTUAL_OS_PROGRAM)
}

pub fn compute_aggregator_program_hash() -> Result<AggregatorHash, ProgramHashError> {
    let hash = compute_program_hash(&AGGREGATOR_PROGRAM)?;
    Ok(AggregatorHash {
        with_prefix: Pedersen::hash(&Felt::from_bytes_be(&pad_to_32_bytes(b"AGGREGATOR")), &hash),
        without_prefix: hash,
    })
}
