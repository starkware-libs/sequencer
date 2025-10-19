use clap::{Parser, Subcommand};
use starknet_os::hints::hint_implementation::kzg::utils::{
    compute_blob_commitments,
    compute_legacy_blob_commitments,
};
use tracing::info;

use crate::shared_utils::read::{load_input, write_to_file};
use crate::shared_utils::types::IoArgs;

#[derive(Parser, Debug)]
pub struct KzgCliCommand {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    ComputeBlobCommitments {
        #[clap(flatten)]
        io_args: IoArgs,
    },
    ComputeLegacyBlobCommitments {
        #[clap(flatten)]
        io_args: IoArgs,
    },
}

pub fn run_kzg_cli(kzg_command: KzgCliCommand) {
    info!("Starting KZG CLI with command: \n{:?}", kzg_command);
    match kzg_command.command {
        Command::ComputeBlobCommitments { io_args: IoArgs { input_path, output_path } } => {
            let raw_blobs: Vec<Vec<u8>> = load_input(input_path);
            let blobs = compute_blob_commitments(raw_blobs)
                .unwrap_or_else(|error| panic!("Failed to calculate blob commitments: {error}"));
            write_to_file(&output_path, &blobs);
        }
        // TODO(Yoni): remove this command once python migrates to the new blob commitments.
        Command::ComputeLegacyBlobCommitments { io_args: IoArgs { input_path, output_path } } => {
            let raw_blobs: Vec<Vec<u8>> = load_input(input_path);
            let blobs = compute_legacy_blob_commitments(raw_blobs).unwrap_or_else(|error| {
                panic!("Failed to calculate legacy blob commitments: {error}")
            });
            write_to_file(&output_path, &blobs);
        }
    };
}
