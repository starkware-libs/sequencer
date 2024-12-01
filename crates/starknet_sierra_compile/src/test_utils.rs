use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::ChildStderr;

use cairo_lang_starknet_classes::contract_class::{ContractClass, ContractEntryPoints};
use cairo_lang_utils::bigint::BigUintAsHex;
use serde::Deserialize;

/// Same as `ContractClass` - but ignores unnecessary fields like `abi` in deserialization.
#[derive(Deserialize)]
struct DeserializedContractClass {
    pub sierra_program: Vec<BigUintAsHex>,
    pub sierra_program_debug_info: Option<cairo_lang_sierra::debug_info::DebugInfo>,
    pub contract_class_version: String,
    pub entry_points_by_type: ContractEntryPoints,
}

pub(crate) fn contract_class_from_file<P: AsRef<Path>>(path: P) -> ContractClass {
    let DeserializedContractClass {
        sierra_program,
        sierra_program_debug_info,
        contract_class_version,
        entry_points_by_type,
    } = serde_json::from_str(&fs::read_to_string(path).expect("Failed to read input file."))
        .expect("deserialization Failed.");

    ContractClass {
        sierra_program,
        sierra_program_debug_info,
        contract_class_version,
        entry_points_by_type,
        abi: None,
    }
}

pub(crate) fn get_memory_usage_kb(pid: u32) -> std::io::Result<u64> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status"))?;
    for line in status.lines() {
        if line.starts_with("VmPeak:") {
            // Example line: "VmPeak:    123456 KB"
            let parts: Vec<&str> = line.split_whitespace().collect();
            if let Some(kb_str) = parts.get(1) {
                if let Ok(kb) = kb_str.parse() {
                    return Ok(kb);
                }
            }
        }
    }
    Ok(0)
}

/// Scans a given stderr for the number of bytes specified in the error message of the form:
/// ```
/// bash: xmalloc: cannot allocate 37097 bytes
/// ```
pub(crate) fn get_xmalloc_error_num_bytes(mut stderr: ChildStderr) -> std::io::Result<u64> {
    let mut stderr_str = String::new();
    stderr.read_to_string(&mut stderr_str)?;
    for line in stderr_str.lines() {
        if line.starts_with("bash: xmalloc: cannot allocate") {
            if let Some(bytes) = line.split_whitespace().nth(4) {
                if let Ok(num_bytes) = bytes.parse() {
                    return Ok(num_bytes);
                }
                break;
            }
        }
    }
    Ok(0)
}
