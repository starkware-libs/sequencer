use std::fs;
use std::path::Path;

use apollo_gateway_config::config::RpcStateReaderConfig;
use blockifier_reexecution::state_reader::cli::{
    parse_block_numbers_args,
    BlockifierReexecutionCliArgs,
    Command,
    FULL_RESOURCES_DIR,
};
use blockifier_reexecution::state_reader::offline_state_reader::OfflineConsecutiveStateReaders;
use blockifier_reexecution::state_reader::test_state_reader::ConsecutiveTestStateReaders;
use blockifier_reexecution::state_reader::utils::{
    execute_single_transaction_from_json,
    reexecute_and_verify_correctness,
    write_block_reexecution_data_to_file,
};
use clap::Parser;
use google_cloud_storage::client::{Client, ClientConfig};
use google_cloud_storage::http::objects::download::Range;
use google_cloud_storage::http::objects::get::GetObjectRequest;
use google_cloud_storage::http::objects::upload::{Media, UploadObjectRequest, UploadType};
use starknet_api::block::BlockNumber;

const BUCKET: &str = "reexecution_artifacts";
const RESOURCES_DIR: &str = "/resources";
const FILE_NAME: &str = "/reexecution_data.json";
const OFFLINE_PREFIX_FILE: &str = "/offline_reexecution_files_prefix";

/// Main entry point of the blockifier reexecution CLI.
/// TODO(Aner): run by default from the root of the project.
#[tokio::main]
async fn main() {
    let args = BlockifierReexecutionCliArgs::parse();

    // Lambda functions for single point of truth.
    let block_dir = |block_number| format!("/block_{block_number}");
    let block_data_file = |block_number| block_dir(block_number) + FILE_NAME;
    let block_full_directory =
        |directory_path: String, block_number| directory_path + &block_dir(block_number);
    let block_full_file_path = |directory_path, block_number| {
        block_full_directory(directory_path, block_number) + FILE_NAME
    };
    let prefix_dir = |directory_path| {
        fs::read_to_string(directory_path + OFFLINE_PREFIX_FILE)
            .expect("Failed to read files' prefix.")
            .trim()
            .to_string()
            + RESOURCES_DIR
    };

    match args.command {
        Command::RpcTest { block_number, rpc_args } => {
            println!(
                "Running RPC test for block number {block_number} using node url {}.",
                rpc_args.node_url
            );

            let config = RpcStateReaderConfig::from_url(rpc_args.node_url.clone());

            // RPC calls are "synchronous IO" (see, e.g., https://stackoverflow.com/questions/74547541/when-should-you-use-tokios-spawn-blocking)
            // for details), so should be executed in a blocking thread.
            // TODO(Aner): make only the RPC calls blocking, not the whole function.
            tokio::task::spawn_blocking(move || {
                reexecute_and_verify_correctness(ConsecutiveTestStateReaders::new(
                    BlockNumber(block_number - 1),
                    Some(config),
                    rpc_args.parse_chain_id(),
                    false,
                ))
            })
            .await
            .unwrap();

            // Compare the expected and actual state differences
            // by avoiding discrepancies caused by insertion order
            println!("RPC test passed successfully.");
        }

        Command::ReExecuteSingleTx { block_number, rpc_args, transaction_path } => {
            println!(
                "Executing single transaction from {transaction_path}, for block number \
                 {block_number}, and using node url {}.",
                rpc_args.node_url
            );

            let (node_url, chain_id) = (rpc_args.node_url.clone(), rpc_args.parse_chain_id());

            // RPC calls are "synchronous IO" (see, e.g., https://stackoverflow.com/questions/74547541/when-should-you-use-tokios-spawn-blocking)
            // for details), so should be executed in a blocking thread.
            // TODO(Aner): make only the RPC calls blocking, not the whole function.
            tokio::task::spawn_blocking(move || {
                execute_single_transaction_from_json(
                    BlockNumber(block_number),
                    node_url,
                    chain_id,
                    transaction_path,
                )
            })
            .await
            .unwrap()
            .unwrap();

            println!("Single transaction execution completed.");
        }

        Command::WriteToFile { block_numbers, directory_path, rpc_args } => {
            let directory_path = directory_path.unwrap_or(FULL_RESOURCES_DIR.to_string());

            let block_numbers = parse_block_numbers_args(block_numbers);
            println!("Computing reexecution data for blocks {block_numbers:?}.");

            let mut task_set = tokio::task::JoinSet::new();
            for block_number in block_numbers {
                let full_file_path = block_full_file_path(directory_path.clone(), block_number);
                let (node_url, chain_id) = (rpc_args.node_url.clone(), rpc_args.parse_chain_id());
                // RPC calls are "synchronous IO" (see, e.g., https://stackoverflow.com/questions/74547541/when-should-you-use-tokios-spawn-blocking)
                // for details), so should be executed in a blocking thread.
                // TODO(Aner): make only the RPC calls blocking, not the whole function.
                task_set.spawn(async move {
                    println!("Computing reexecution data for block {block_number}.");
                    tokio::task::spawn_blocking(move || {
                        write_block_reexecution_data_to_file(
                            block_number,
                            full_file_path,
                            node_url,
                            chain_id,
                        )
                    })
                    .await
                });
            }
            println!("Waiting for all blocks to be processed.");
            task_set.join_all().await;
        }

        Command::Reexecute { block_numbers, directory_path } => {
            let directory_path = directory_path.unwrap_or(FULL_RESOURCES_DIR.to_string());

            let block_numbers = parse_block_numbers_args(block_numbers);
            println!("Reexecuting blocks {block_numbers:?}.");

            let mut task_set = tokio::task::JoinSet::new();
            for block in block_numbers {
                let full_file_path = block_full_file_path(directory_path.clone(), block);
                task_set.spawn(async move {
                    reexecute_and_verify_correctness(
                        OfflineConsecutiveStateReaders::new_from_file(&full_file_path).unwrap(),
                    );
                    println!("Reexecution test for block {block} passed successfully.");
                });
            }
            println!("Waiting for all blocks to be processed.");
            task_set.join_all().await;
        }

        // Uploading the files requires authentication; please run
        // `gcloud auth application-default login` in terminal before running this command.
        Command::UploadFiles { block_numbers, directory_path } => {
            let directory_path = directory_path.unwrap_or(FULL_RESOURCES_DIR.to_string());

            let block_numbers = parse_block_numbers_args(block_numbers);
            println!("Uploading blocks {block_numbers:?}.");

            let files_prefix = prefix_dir(directory_path.clone());

            // Get the client with authentication.
            let config = ClientConfig::default()
                .with_auth()
                .await
                .expect("Failed to get client. Please run `gcloud auth application-default login`");
            let client = Client::new(config);

            // Verify all required files exist locally, and do not exist in the gc bucket.
            for block_number in block_numbers.clone() {
                assert!(
                    Path::exists(Path::new(&block_full_file_path(
                        directory_path.clone(),
                        block_number
                    ))),
                    "Block {block_number} reexecution data file does not exist."
                );
                assert!(
                    client
                        .get_object(&GetObjectRequest {
                            bucket: BUCKET.to_string(),
                            object: files_prefix.clone()
                                + &block_data_file(block_number),
                            ..Default::default()
                        })
                        .await
                        // TODO(Aner): check that the error is not found error.
                        .is_err(),
                    "Block {block_number} reexecution data file already exists in bucket."
                )
            }

            // Upload all files to the gc bucket.
            for block_number in block_numbers {
                client
                    .upload_object(
                        &UploadObjectRequest { bucket: BUCKET.to_string(), ..Default::default() },
                        fs::read(block_full_file_path(directory_path.clone(), block_number))
                            .unwrap(),
                        &UploadType::Simple(Media::new(
                            files_prefix.clone() + &block_data_file(block_number),
                        )),
                    )
                    .await
                    .unwrap();
            }

            println!(
                "All blocks uploaded successfully to https://console.cloud.google.com/storage/browser/{BUCKET}/{files_prefix}."
            );
        }

        Command::DownloadFiles { block_numbers, directory_path } => {
            let directory_path = directory_path.unwrap_or(FULL_RESOURCES_DIR.to_string());

            let block_numbers = parse_block_numbers_args(block_numbers);
            println!("Downloading blocks {block_numbers:?}.");

            let files_prefix = prefix_dir(directory_path.clone());

            // Get the client with authentication.
            let config = ClientConfig::default()
                .with_auth()
                .await
                .expect("Failed to get client. Please run `gcloud auth application-default login`");
            let client = Client::new(config);

            // Download all files from the gc bucket.
            for block_number in block_numbers {
                let res = client
                    .download_object(
                        &GetObjectRequest {
                            bucket: BUCKET.to_string(),
                            object: files_prefix.clone() + &block_data_file(block_number),
                            ..Default::default()
                        },
                        &Range::default(),
                    )
                    .await
                    .unwrap();
                fs::create_dir_all(block_full_directory(directory_path.clone(), block_number))
                    .unwrap();
                fs::write(block_full_file_path(directory_path.clone(), block_number), res).unwrap();
            }

            println!("All blocks downloaded successfully to {directory_path}.");
        }
    }
}
