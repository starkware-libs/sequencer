use cairo_vm::program_hash::{compute_program_hash_chain, ProgramHashError as VmProgramHashError};
use cairo_vm::types::errors::program_errors::ProgramError;
use cairo_vm::types::program::Program;
use serde::{Deserialize, Serialize};
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::{Pedersen, StarkHash};

use crate::{AGGREGATOR_PROGRAM, OS_PROGRAM};

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

pub fn compute_os_program_hash() -> Result<Felt, ProgramHashError> {
    compute_program_hash(&OS_PROGRAM)
}

pub fn compute_aggregator_program_hash() -> Result<AggregatorHash, ProgramHashError> {
    let hash = compute_program_hash(&AGGREGATOR_PROGRAM)?;
    Ok(AggregatorHash {
        with_prefix: Pedersen::hash(&Felt::from_bytes_be(&pad_to_32_bytes(b"AGGREGATOR")), &hash),
        without_prefix: hash,
    })
}
