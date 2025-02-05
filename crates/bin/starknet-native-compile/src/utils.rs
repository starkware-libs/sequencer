use std::path::PathBuf;
use std::process;

use cairo_lang_sierra::program::Program;
use cairo_lang_starknet_classes::compiler_version::VersionId;
use cairo_lang_starknet_classes::contract_class::ContractClass;

pub(crate) fn get_sierra_verizon_from_program<F>(sierra_program: &[F]) -> VersionId
where
    F: TryInto<usize> + std::fmt::Display + Clone,
    <F as TryInto<usize>>::Error: std::fmt::Display,
{
    if sierra_program.len() < 3 {
        eprintln!("Sierra program length must be at least 3 Felts.",);
        process::exit(1);
    }

    let version_components: Vec<usize> = sierra_program
        .iter()
        .take(3)
        .enumerate()
        .map(|(index, felt)| {
            felt.clone().try_into().map_err(|err| {
                eprintln!(
                    "Failed to parse Sierra program to Sierra version. Index: {}, Felt: {}, \
                     Error: {}",
                    index, felt, err
                );
                process::exit(1);
            })
        })
        .collect::<Result<_, _>>()
        .unwrap_or_else(|_| {
            eprintln!("Error parsing Sierra program to Sierra version");
            process::exit(1);
        });
    VersionId {
        major: version_components[0],
        minor: version_components[1],
        patch: version_components[2],
    }
}

pub(crate) fn load_sierra_program_from_file(path: &PathBuf) -> (ContractClass, Program) {
    let raw_contract_class = std::fs::read_to_string(path).unwrap_or_else(|err| {
        eprintln!("Error reading Sierra file: {}", err);
        process::exit(1);
    });

    let contract_class: ContractClass =
        serde_json::from_str(&raw_contract_class).unwrap_or_else(|err| {
            eprintln!("Error deserializing Sierra file into contract class: {}", err);
            process::exit(1);
        });
    (
        contract_class.clone(),
        contract_class.extract_sierra_program().unwrap_or_else(|err| {
            eprintln!("Error extracting Sierra program from contract class: {}", err);
            process::exit(1);
        }),
    )
}
