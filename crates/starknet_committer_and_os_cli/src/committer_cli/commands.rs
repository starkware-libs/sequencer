use starknet_committer::block_committer::commit::{CommitBlockImpl, CommitBlockTrait};
use starknet_committer::block_committer::measurements_util::NoMeasurements;
use starknet_committer::db::facts_db::FactsDb;
use starknet_committer::db::forest_trait::StorageInitializer;
use starknet_patricia_storage::map_storage::MapStorage;
use tracing::info;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::reload::Handle;
use tracing_subscriber::Registry;

use crate::committer_cli::filled_tree_output::filled_forest::SerializedForest;
use crate::committer_cli::parse_input::cast::{CommitterFactsDbInputImpl, FactsDbInputImpl};
use crate::committer_cli::parse_input::raw_input::RawInput;
use crate::shared_utils::read::{load_input, write_to_file};

pub async fn parse_and_commit(
    input_path: String,
    output_path: String,
    log_filter_handle: Handle<LevelFilter, Registry>,
) {
    let CommitterFactsDbInputImpl { input, log_level, storage } =
        load_input::<RawInput>(input_path)
            .try_into()
            .expect("Failed to convert RawInput to FactsDbInputImpl.");
    info!(
        "Parsed committer input successfully. Original Contracts Trie Root Hash: {:?},
    Original Classes Trie Root Hash: {:?}",
        input.initial_read_context.0.contracts_trie_root_hash,
        input.initial_read_context.0.classes_trie_root_hash,
    );
    // Set the given log level if handle is passed.
    log_filter_handle.modify(|filter| *filter = log_level).expect("Failed to set the log level.");
    commit(input, output_path, storage).await;
}

pub async fn commit(input: FactsDbInputImpl, output_path: String, storage: MapStorage) {
    let mut facts_db = FactsDb::new(storage);
    let serialized_filled_forest = SerializedForest(
        CommitBlockImpl::commit_block(input, &mut facts_db, &mut NoMeasurements)
            .await
            .expect("Failed to commit the given block."),
    );
    let output = serialized_filled_forest
        .forest_to_output()
        .await
        .expect("Failed to serialize filled forest");
    write_to_file(&output_path, &output);
    info!(
        "Successfully committed given block. Updated Contracts Trie Root Hash: {:?},
    Updated Classes Trie Root Hash: {:?}",
        output.contract_storage_root_hash, output.compiled_class_root_hash,
    );
}
