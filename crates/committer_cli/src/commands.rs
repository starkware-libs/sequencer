use committer::block_committer::{
    commit::commit_block,
    input::{ConfigImpl, Input},
};

use crate::{
    filled_tree_output::filled_forest::SerializedForest, parse_input::read::write_to_file,
};

pub async fn commit(input: Input<ConfigImpl>, output_path: String) {
    let serialized_filled_forest = SerializedForest(
        commit_block(input)
            .await
            .expect("Failed to commit the given block."),
    );
    let output = serialized_filled_forest.forest_to_output();
    write_to_file(&output_path, &output);
}
