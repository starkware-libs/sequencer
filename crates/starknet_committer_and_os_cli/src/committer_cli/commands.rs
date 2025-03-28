use starknet_committer::block_committer::commit::commit_block;
use starknet_committer::block_committer::input::{Config, ConfigImpl, Input};
use tracing::info;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::reload::Handle;
use tracing_subscriber::Registry;

use crate::committer_cli::filled_tree_output::filled_forest::SerializedForest;
use crate::committer_cli::parse_input::cast::InputImpl;
use crate::committer_cli::parse_input::raw_input::RawInput;
use crate::shared_utils::read::{load_input, write_to_file};

pub async fn parse_and_commit(
    input_path: String,
    output_path: String,
    log_filter_handle: Handle<LevelFilter, Registry>,
) {
    let input: InputImpl = load_input::<RawInput>(input_path)
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
    commit(input, output_path).await;
}

pub async fn commit(input: Input<ConfigImpl>, output_path: String) {
    let serialized_filled_forest =
        SerializedForest(commit_block(input).await.expect("Failed to commit the given block."));
    let output = serialized_filled_forest.forest_to_output();
    write_to_file(&output_path, &output);
    info!(
        "Successfully committed given block. Updated Contracts Trie Root Hash: {:?},
    Updated Classes Trie Root Hash: {:?}",
        output.contract_storage_root_hash, output.compiled_class_root_hash,
    );
}
