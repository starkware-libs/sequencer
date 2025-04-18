use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::errors::program_errors::ProgramError;
use serde::{Deserialize, Serialize};
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::{Pedersen, StarkHash};

use crate::OS_PROGRAM;

#[cfg(test)]
#[path = "program_hash_test.rs"]
pub mod test;

#[derive(thiserror::Error, Debug)]
pub enum ProgramHashError {
    #[error("Builtin name is too long: {0}.")]
    BuiltinNameTooLong(BuiltinName),
    #[error(transparent)]
    Program(#[from] ProgramError),
    #[error("Program data contains unexpected relocatable.")]
    UnexpectedRelocatable,
}

#[derive(Deserialize, Serialize)]
pub struct ProgramHash {
    os: Felt,
}

const BOOTLOADER_VERSION: u8 = 0;

fn pedersen_hash_chain(data: Vec<Felt>) -> Felt {
    let length = Felt::from(data.len());
    vec![length]
        .into_iter()
        .chain(data)
        .rev()
        .reduce(|x, y| Pedersen::hash(&y, &x))
        .expect("Hash data chain is not empty.")
}

pub fn compute_os_program_hash() -> Result<Felt, ProgramHashError> {
    let builtins = OS_PROGRAM
        .iter_builtins()
        .map(|builtin| {
            let builtin_bytes = builtin.to_str().to_string().into_bytes();
            if builtin_bytes.len() > 32 {
                Err(ProgramHashError::BuiltinNameTooLong(*builtin))
            } else {
                let mut padded_builtin_bytes = [0].repeat(32 - builtin_bytes.len());
                padded_builtin_bytes.extend(builtin_bytes);
                Ok(Felt::from_bytes_be(
                    padded_builtin_bytes
                        .as_slice()
                        .try_into()
                        .expect("Padded bytes are 32 bytes long."),
                ))
            }
        })
        .collect::<Result<Vec<Felt>, _>>()?;
    let program_header = vec![
        Felt::from(BOOTLOADER_VERSION),
        Felt::from(OS_PROGRAM.get_stripped_program()?.main),
        Felt::from(builtins.len()),
    ];
    let data = OS_PROGRAM
        .iter_data()
        .map(|data| data.get_int().ok_or(ProgramHashError::UnexpectedRelocatable))
        .collect::<Result<Vec<Felt>, _>>()?;

    let data_chain: Vec<Felt> = program_header.into_iter().chain(builtins).chain(data).collect();
    Ok(pedersen_hash_chain(data_chain))
}
