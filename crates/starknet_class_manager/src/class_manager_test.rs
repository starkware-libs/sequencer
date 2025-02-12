use std::sync::Arc;

use mockall::predicate::eq;
use rstest::rstest;
use starknet_api::core::CompiledClassHash;
use starknet_api::felt;
use starknet_api::state::SierraContractClass;
use starknet_class_manager_types::ClassHashes;
use starknet_sierra_multicompile_types::{MockSierraCompilerClient, RawClass, RawExecutableClass};

use crate::class_manager::ClassManager;
use crate::class_storage::{create_tmp_dir, CachedClassStorageConfig, FsClassStorage};
use crate::config::ClassManagerConfig;

impl ClassManager<FsClassStorage> {
    fn new_for_testing(
        compiler: MockSierraCompilerClient,
        persistent_root: &tempfile::TempDir,
        class_hash_storage_path_prefix: &tempfile::TempDir,
    ) -> Self {
        let cached_class_storage_config =
            CachedClassStorageConfig { class_cache_size: 10, deprecated_class_cache_size: 10 };
        let config = ClassManagerConfig { cached_class_storage_config };
        let storage =
            FsClassStorage::new_for_testing(persistent_root, class_hash_storage_path_prefix);

        ClassManager::new(config, Arc::new(compiler), storage)
    }
}

#[tokio::test]
async fn class_manager() {
    // Setup.

    // Prepare mock compiler.
    let mut compiler = MockSierraCompilerClient::new();
    let class = RawClass::try_from(SierraContractClass::default()).unwrap();
    let expected_executable_class = RawExecutableClass(vec![4, 5, 6].into());
    let expected_executable_class_for_closure = expected_executable_class.clone();
    let expected_executable_class_hash = CompiledClassHash(felt!("0x5678"));
    compiler.expect_compile().with(eq(class.clone())).times(1).return_once(move |_| {
        Ok((expected_executable_class_for_closure, expected_executable_class_hash))
    });

    // Prepare class manager.
    let persistent_root = create_tmp_dir().unwrap();
    let class_hash_storage_path_prefix = create_tmp_dir().unwrap();
    let mut class_manager =
        ClassManager::new_for_testing(compiler, &persistent_root, &class_hash_storage_path_prefix);

    // Test.

    // Non-existent class.
    let class_id = SierraContractClass::try_from(class.clone()).unwrap().calculate_class_hash();
    assert_eq!(class_manager.get_sierra(class_id), Ok(None));
    assert_eq!(class_manager.get_executable(class_id), Ok(None));

    // Add new class.
    let class_hashes = class_manager.add_class(class.clone()).await.unwrap();
    let expected_class_hashes =
        ClassHashes { class_hash: class_id, executable_class_hash: expected_executable_class_hash };
    assert_eq!(class_hashes, expected_class_hashes);

    // Get class.
    assert_eq!(class_manager.get_sierra(class_id).unwrap(), Some(class.clone()));
    assert_eq!(class_manager.get_executable(class_id).unwrap(), Some(expected_executable_class));

    // Add existing class; response returned immediately, without invoking compilation.
    let class_hashes = class_manager.add_class(class).await.unwrap();
    assert_eq!(class_hashes, expected_class_hashes);
}

use tokio::runtime::{Handle, Runtime};

async fn async_function() -> i32 {
    // Your async logic here
    println!("Hello from async!");
    42
}

#[rstest]
fn call_async_from_sync() {
    // Create a new runtime
    let rt = Runtime::new().expect("Failed to create Tokio runtime");

    // Block on the async function, waiting for it to complete
    let result = rt.block_on(async_function());
    println!("Result: {}", result);
}

fn call_async_from_sync_handle_aux() {
    let handle = Handle::current();
    handle.spawn(async {
        let result = async_function().await;
        println!("(tokio::spawn) Async returned: {}", result);
        result
    });
}

// #[rstest]
// fn call_async_from_sync_handle() {
//     let join_handle = call_async_from_sync_tokio();
//     let value = join_handle.await.unwrap();
// }

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

// This async function simulates some work by sleeping for 3 seconds
async fn my_async_function() -> i32 {
    // Use Tokio's sleep to simulate asynchronous work.
    tokio::time::sleep(Duration::from_secs(3)).await;
    42
}

#[rstest]
fn main() {
    // Create a channel to receive the result from the async task.
    let (tx, rx) = mpsc::channel();

    // Spawn a new thread to run the async task.
    thread::spawn(move || {
        // Create a new Tokio runtime on this separate thread.
        let rt = Runtime::new().expect("Failed to create Tokio runtime");

        // Spawn the async task on the runtime.
        let join_handle = rt.spawn(async { my_async_function().await });

        // Use block_on on the runtime to await the join handle.
        // This blocks *this* thread only, not the main thread.
        let result = rt.block_on(async { join_handle.await.expect("Task panicked") });

        // Send the result back to the main thread.
        tx.send(result).expect("Failed to send result");
    });

    // Main thread: printing loop that remains responsive while the async task runs.
    loop {
        // Try to receive the result without blocking.
        if let Ok(result) = rx.try_recv() {
            println!("Async function completed with result: {}", result);
            break;
        } else {
            println!("Main thread is still working...");
        }
        // Sleep a bit before checking again.
        thread::sleep(Duration::from_millis(500));
    }

    println!("Main thread exiting.");
}
