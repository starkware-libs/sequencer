use starknet_committer::block_committer::commit::commit_block;
use starknet_committer::block_committer::input::{Config, ConfigImpl, Input};

use crate::filled_tree_output::filled_forest::SerializedForest;
use crate::parse_input::read::{parse_input, write_to_file};

pub async fn parse_and_commit(input_string: &str, output_path: String) {
    let input = parse_input(input_string).expect("Failed to parse the given input.");
    // Set the given log level.
    log::set_max_level(input.config.logger_level());
    commit(input, output_path).await;
}

pub async fn commit(input: Input<ConfigImpl>, output_path: String) {
    let serialized_filled_forest =
        SerializedForest(commit_block(input).await.expect("Failed to commit the given block."));
    let output = serialized_filled_forest.forest_to_output();
    write_to_file(&output_path, &output);
}
