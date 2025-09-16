//! State Diff Hash Calculator
//!
//! This utility calculates the state diff hash (which is reported by consensus logs: "Agreed on
//! block: 0x...") for a state update (what's available in block explorer in the feeder).
//!
//! ## Usage
//!
//! The program reads a JSON-encoded StateUpdate from stdin and outputs the calculated
//! state diff hash to stdout.
//!
//! ```bash
//! # Read from a file
//! cargo run --bin state_diff_hash_calculator < state_update.json
//!
//! # Or use cat to pipe from a file
//! cat state_update.json | cargo run --bin state_diff_hash_calculator
//! ```
//!
//! ## Input Format
//!
//! The input should be a JSON object containing a `StateUpdate` with the following structure:
//! - `state_diff`: Contains deployed contracts, storage diffs, declared classes, etc.

use std::io::{self, Read};

use apollo_starknet_client::reader::objects::state::StateDiff;
use apollo_starknet_client::reader::StateUpdate;
use starknet_api::block_hash::state_diff_hash::calculate_state_diff_hash;
use starknet_api::state::ThinStateDiff;

/// Convert from apollo_starknet_client StateDiff to starknet_api ThinStateDiff
fn convert_to_thin_state_diff(client_state_diff: StateDiff) -> ThinStateDiff {
    ThinStateDiff {
        deployed_contracts: client_state_diff
            .deployed_contracts
            .into_iter()
            .map(|deployed_contract| (deployed_contract.address, deployed_contract.class_hash))
            .collect(),
        storage_diffs: client_state_diff
            .storage_diffs
            .into_iter()
            .map(|(address, storage_entries)| {
                let storage_map =
                    storage_entries.into_iter().map(|entry| (entry.key, entry.value)).collect();
                (address, storage_map)
            })
            .collect(),
        class_hash_to_compiled_class_hash: client_state_diff
            .declared_classes
            .into_iter()
            .map(|declared_class| (declared_class.class_hash, declared_class.compiled_class_hash))
            .collect(),
        deprecated_declared_classes: client_state_diff.old_declared_contracts,
        nonces: client_state_diff.nonces,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Read all input from stdin until EOF.
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;

    let state_update: StateUpdate = serde_json::from_str(&input)?;

    let thin_state_diff = convert_to_thin_state_diff(state_update.state_diff);

    let hash = calculate_state_diff_hash(&thin_state_diff);

    println!("State diff hash: {:?}", hash.0.0);

    Ok(())
}
