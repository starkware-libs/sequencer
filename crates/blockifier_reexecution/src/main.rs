use std::fs;
use std::path::Path;

use apollo_gateway_config::config::RpcStateReaderConfig;
use blockifier_reexecution::state_reader::cli::{
    BlockifierReexecutionCliArgs, Command, FULL_RESOURCES_DIR, TransactionInput,
    parse_block_numbers_args,
};
use blockifier_reexecution::state_reader::offline_state_reader::OfflineConsecutiveStateReaders;
use blockifier_reexecution::state_reader::test_state_reader::ConsecutiveTestStateReaders;
use blockifier_reexecution::state_reader::utils::{
    execute_single_transaction, reexecute_and_verify_correctness,
    write_block_reexecution_data_to_file,
};
#[cfg(feature = "cairo_native")]
use blockifier_reexecution::state_reader::utils::create_native_config_for_reexecution;
#[cfg(feature = "cairo_native")]
use blockifier_reexecution::state_reader::utils::reexecute_and_verify_correctness_with_native;
use clap::Parser;
use google_cloud_storage::client::{Client, ClientConfig};
use google_cloud_storage::http::objects::download::Range;
use google_cloud_storage::http::objects::get::GetObjectRequest;
use google_cloud_storage::http::objects::upload::{Media, UploadObjectRequest, UploadType};
use num_bigint::BigUint;
use num_traits::Num;
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

    // Set log level to debug.
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{filter, fmt, reload};
    let (global_filter, _global_filter_handle) = reload::Layer::new(filter::LevelFilter::INFO);
    let layer = fmt::Layer::default()
        .with_ansi(false)
        .with_target(false)
        .with_file(true)
        .with_line_number(true);
    tracing_subscriber::registry().with(global_filter).with(layer).init();

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
        Command::RpcTest { block_number, rpc_args, run_cairo_native } => {
            println!(
                "Running RPC test for block number {block_number} using node url {}{}.",
                rpc_args.node_url,
                if run_cairo_native { " with Cairo native" } else { "" }
            );

            let config = RpcStateReaderConfig::from_url(rpc_args.node_url.clone());

            // RPC calls are "synchronous IO" (see, e.g., https://stackoverflow.com/questions/74547541/when-should-you-use-tokios-spawn-blocking)
            // for details), so should be executed in a blocking thread.
            // TODO(Aner): make only the RPC calls blocking, not the whole function.
            tokio::task::spawn_blocking(move || {
                let consecutive_state_readers = ConsecutiveTestStateReaders::new(
                    BlockNumber(block_number - 1),
                    Some(config),
                    rpc_args.parse_chain_id(),
                    false,
                );

                #[cfg(feature = "cairo_native")]
                if run_cairo_native {
                    reexecute_and_verify_correctness_with_native(
                        consecutive_state_readers,
                        create_native_config_for_reexecution(true, true),
                    );
                    return;
                }

                #[cfg(not(feature = "cairo_native"))]
                if run_cairo_native {
                    panic!(
                        "Cairo native feature is not enabled. Rebuild with --features cairo_native"
                    );
                }

                reexecute_and_verify_correctness(consecutive_state_readers);
            })
            .await
            .unwrap();

            // Compare the expected and actual state differences
            // by avoiding discrepancies caused by insertion order
            println!("RPC test passed successfully.");
        }

        Command::ReexecuteSingleTx { block_number, rpc_args, run_cairo_native, tx_input } => {
            let tx_source = match tx_input {
                TransactionInput::FromFile { ref tx_path } => format!("from file {tx_path}"),
                TransactionInput::FromHash { ref tx_hash } => format!("with hash {tx_hash}"),
            };

            println!(
                "Executing single transaction {tx_source}, for block number {block_number}, and \
                 using node url {}{}.",
                rpc_args.node_url,
                if run_cairo_native { " with Cairo native" } else { "" }
            );

            let (node_url, chain_id) = (rpc_args.node_url.clone(), rpc_args.parse_chain_id());

            // RPC calls are "synchronous IO" (see, e.g., https://stackoverflow.com/questions/74547541/when-should-you-use-tokios-spawn-blocking)
            // for details), so should be executed in a blocking thread.
            // TODO(Aner): make only the RPC calls blocking, not the whole function.
            tokio::task::spawn_blocking(move || {
                execute_single_transaction(
                    BlockNumber(block_number),
                    node_url,
                    chain_id,
                    tx_input,
                    run_cairo_native,
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
            tracing::info!("Computing reexecution data for blocks {block_numbers:?}.");

            let mut task_set = tokio::task::JoinSet::new();
            for block_number in block_numbers {
                let full_file_path = block_full_file_path(directory_path.clone(), block_number);
                let (node_url, chain_id) =
                    (rpc_args.node_url.clone(), rpc_args.chain_id.clone().unwrap().into());
                // RPC calls are "synchronous IO" (see, e.g., https://stackoverflow.com/questions/74547541/when-should-you-use-tokios-spawn-blocking
                // for details), so should be executed in a blocking thread.
                // TODO(Aner): make only the RPC calls blocking, not the whole function.
                task_set.spawn(async move {
                    tracing::info!("Computing reexecution data for block {block_number}.");
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
            tracing::info!("Waiting for all blocks to be processed.");
            task_set.join_all().await;
        }

        Command::Reexecute { block_numbers, directory_path, run_cairo_native } => {
            let directory_path = directory_path.unwrap_or(FULL_RESOURCES_DIR.to_string());

            let block_numbers = parse_block_numbers_args(block_numbers);
            println!(
                "Reexecuting blocks {block_numbers:?}{}.",
                if run_cairo_native { " with Cairo native" } else { "" }
            );

            #[cfg(not(feature = "cairo_native"))]
            if run_cairo_native {
                panic!("Cairo native feature is not enabled. Rebuild with --features cairo_native");
            }

            let mut task_set = tokio::task::JoinSet::new();
            for block in block_numbers {
                let full_file_path = block_full_file_path(directory_path.clone(), block);
                task_set.spawn(async move {
                    let consecutive_state_readers =
                        OfflineConsecutiveStateReaders::new_from_file(&full_file_path).unwrap();

                    #[cfg(feature = "cairo_native")]
                    if run_cairo_native {
                        reexecute_and_verify_correctness_with_native(
                            consecutive_state_readers,
                            create_native_config_for_reexecution(true, true),
                        );
                        println!(
                            "Reexecution test for block {block} passed successfully (with Cairo \
                             native)."
                        );
                        return;
                    }

                    reexecute_and_verify_correctness(consecutive_state_readers);
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
