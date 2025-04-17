use cairo_vm::types::builtin_name::BuiltinName;
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
    #[error("Builtin name of builtin {builtin} is too long: '{name}'.")]
    BuiltinNameTooLong { builtin: BuiltinName, name: String },
    #[error(transparent)]
    Program(#[from] ProgramError),
    #[error("Program data contains unexpected relocatable.")]
    UnexpectedRelocatable,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct ProgramHash {
    pub os: Felt,
    pub aggregator: Felt,
    pub aggregator_with_prefix: Felt,
}

pub struct AggregatorHash {
    pub with_prefix: Felt,
    pub without_prefix: Felt,
}

const BOOTLOADER_VERSION: u8 = 0;

fn pad_to_32_bytes(data: &[u8]) -> [u8; 32] {
    let mut padded = [0; 32];
    let len = data.len();
    if len > 32 {
        panic!("Data length exceeds 32 bytes.");
    }
    padded[32 - len..].copy_from_slice(data);
    padded
}

fn pedersen_hash_chain(data: Vec<Felt>) -> Felt {
    let length = Felt::from(data.len());
    vec![length]
        .into_iter()
        .chain(data)
        .rev()
        .reduce(|x, y| Pedersen::hash(&y, &x))
        .expect("Hash data chain is not empty.")
}

fn compute_program_hash(program: &Program) -> Result<Felt, ProgramHashError> {
    let builtins = program
        .iter_builtins()
        .map(|builtin| {
            let builtin_bytes = builtin.to_str().to_string().into_bytes();
            if builtin_bytes.len() > 32 {
                Err(ProgramHashError::BuiltinNameTooLong {
                    builtin: *builtin,
                    name: builtin.to_str().to_string(),
                })
            } else {
                Ok(Felt::from_bytes_be(&pad_to_32_bytes(&builtin_bytes)))
            }
        })
        .collect::<Result<Vec<Felt>, _>>()?;
    let program_header = vec![
        Felt::from(BOOTLOADER_VERSION),
        // TODO(Dori): When [available](https://github.com/lambdaclass/cairo-vm/pull/2101), use the
        //   Program::get_main() getter instead of the get_stripped_program() method.
        Felt::from(program.get_stripped_program()?.main),
        Felt::from(builtins.len()),
    ];
    let data = program
        .iter_data()
        .map(|data| data.get_int().ok_or(ProgramHashError::UnexpectedRelocatable))
        .collect::<Result<Vec<Felt>, _>>()?;

    let data_chain: Vec<Felt> = program_header.into_iter().chain(builtins).chain(data).collect();
    Ok(pedersen_hash_chain(data_chain))
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
