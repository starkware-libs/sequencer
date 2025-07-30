use std::collections::HashMap;

use starknet_committer::block_committer::commit::commit_block;
use starknet_committer::block_committer::input::Config;
use starknet_patricia_storage::map_storage::BorrowedMapStorage;
use starknet_patricia_storage::storage_trait::{DbKey, DbValue};
use tracing::info;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::reload::Handle;
use tracing_subscriber::Registry;

use crate::committer_cli::filled_tree_output::filled_forest::SerializedForest;
use crate::committer_cli::parse_input::cast::{CommitterInputImpl, InputImpl};
use crate::committer_cli::parse_input::raw_input::RawInput;
use crate::shared_utils::read::{load_input, write_to_file};

pub async fn parse_and_commit(
    input_path: String,
    output_path: String,
    log_filter_handle: Handle<LevelFilter, Registry>,
) {
    let CommitterInputImpl { input, storage } = load_input::<RawInput>(input_path)
        .try_into()
        .expect("Failed to convert RawInput to InputImpl.");
    info!(
        "Parsed committer input successfully. Original Contracts Trie Root Hash: {:?},
    Original Classes Trie Root Hash: {:?}",
        input.contracts_trie_root_hash, input.classes_trie_root_hash,
    );
    // Set the given log level if handle is passed.
    log_filter_handle
        .modify(|filter| *filter = input.config.logger_level())
        .expect("Failed to set the log level.");
    commit(input, output_path, storage).await;
}

pub async fn commit(input: InputImpl, output_path: String, mut storage: HashMap<DbKey, DbValue>) {
    let serialized_filled_forest = SerializedForest(
        commit_block(input, &mut storage).await.expect("Failed to commit the given block."),
    );
    let storage = BorrowedMapStorage { storage: &mut storage };
    let output = serialized_filled_forest.forest_to_output(storage);
    write_to_file(&output_path, &output);
    info!(
        "Successfully committed given block. Updated Contracts Trie Root Hash: {:?},
    Updated Classes Trie Root Hash: {:?}",
        output.contract_storage_root_hash, output.compiled_class_root_hash,
    );
}
