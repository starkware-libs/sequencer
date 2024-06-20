use committer::block_committer::commit::commit_block;

use crate::{filled_tree_output::filled_forest::SerializedForest, parse_input::read::parse_input};

pub async fn commit(input_string: &str) {
    let input = parse_input(input_string).expect("Failed to parse the given input.");
    let serialized_filled_forest = SerializedForest(
        commit_block(input)
            .await
            .expect("Failed to commit the given block."),
    );
    serialized_filled_forest
        .forest_to_python()
        .expect("Failed to print new facts to python.");
}
