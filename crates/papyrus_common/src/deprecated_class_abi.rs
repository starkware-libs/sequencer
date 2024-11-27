use serde::Serialize;

use crate::python_json::PythonJsonFormatter;

// TODO: Consider moving to SN API as a method of deprecated_contract_class::ContractClass.
pub fn calculate_deprecated_class_abi_length(
    deprecated_class: &starknet_api::deprecated_contract_class::ContractClass,
) -> Result<usize, serde_json::Error> {
    let Some(abi) = deprecated_class.abi.as_ref() else {
        return Ok(0);
    };
    let mut chars = vec![];
    abi.serialize(&mut serde_json::Serializer::with_formatter(&mut chars, PythonJsonFormatter))?;
    Ok(chars.len())
}
