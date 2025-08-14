use std::sync::LazyLock;

use starknet_types_core::felt::Felt;

use crate::abi::abi_utils::selector_from_name;
use crate::core::{ascii_as_felt, EntryPointSelector};

pub const EXECUTE_ENTRY_POINT_NAME: &str = "__execute__";
pub const TRANSFER_ENTRY_POINT_NAME: &str = "transfer";
pub const VALIDATE_ENTRY_POINT_NAME: &str = "__validate__";
pub const VALIDATE_DECLARE_ENTRY_POINT_NAME: &str = "__validate_declare__";
pub const VALIDATE_DEPLOY_ENTRY_POINT_NAME: &str = "__validate_deploy__";
pub const DEPLOY_CONTRACT_FUNCTION_ENTRY_POINT_NAME: &str = "deploy_contract";

pub const TRANSFER_EVENT_NAME: &str = "Transfer";

// Cairo constants.
pub const FELT_FALSE: u64 = 0;
pub const FELT_TRUE: u64 = 1;

// Expected return value of a `validate` entry point: `VALID`.
pub static VALIDATE_RETDATA: LazyLock<Felt> =
    LazyLock::new(|| ascii_as_felt("VALID").expect("Failed to parse ASCII"));

pub static VALIDATE_DEPLOY_ENTRY_POINT_SELECTOR: LazyLock<EntryPointSelector> =
    LazyLock::new(|| selector_from_name(VALIDATE_DEPLOY_ENTRY_POINT_NAME));
