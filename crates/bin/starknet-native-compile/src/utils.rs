use std::path::PathBuf;

use anyhow::{anyhow, bail, Context};
use cairo_lang_sierra::program::Program;
use cairo_lang_starknet_classes::compiler_version::VersionId;
use cairo_lang_starknet_classes::contract_class::ContractClass;

// TODO(Meshi): Find a way to avoid this code duplication.
fn get_sierra_version_from_program<F>(sierra_program: &[F]) -> anyhow::Result<VersionId>
where
    F: TryInto<usize> + std::fmt::Display + Clone,
    <F as TryInto<usize>>::Error: std::fmt::Display,
{
    if sierra_program.len() < 3 {
        bail!("Sierra program length must be at least 3 Felts.");
    }

    let version_components: Vec<usize> = sierra_program
        .iter()
        .take(3)
        .enumerate()
        .map(|(index, felt)| {
            felt.clone().try_into().map_err(|err| {
                anyhow!(
                    "Failed to parse Sierra program to Sierra version. Index: {}, Felt: {}, \
                     Error: {}",
                    index,
                    felt,
                    err
                )
            })
        })
        .collect::<Result<_, _>>()?;

    Ok(VersionId {
        major: version_components[0],
        minor: version_components[1],
        patch: version_components[2],
    })
}

pub(crate) fn load_sierra_program_from_file(
    path: &PathBuf,
) -> anyhow::Result<(ContractClass, Program, VersionId)> {
    let raw_contract_class = std::fs::read_to_string(path).context("Error reading Sierra file.")?;

    let contract_class: ContractClass = serde_json::from_str(&raw_contract_class)
        .context("Error deserializing Sierra file into contract class.")?;
    let raw_sierra_program: Vec<_> = contract_class
        .sierra_program
        .iter()
        .map(|big_uint_as_hex| big_uint_as_hex.value.clone())
        .collect();

    let sierra_version = get_sierra_version_from_program(&raw_sierra_program)?;
    Ok((
        contract_class.clone(),
        contract_class
            .extract_sierra_program()
            .context("Error extracting Sierra program from contract class.")?,
        sierra_version,
    ))
}
