use futures::channel::mpsc;
use futures::future::ready;
use futures::FutureExt;
use papyrus_storage::test_utils::get_test_storage;
use papyrus_sync::StateSyncError as PapyrusStateSyncError;
use starknet_sequencer_infra::component_definitions::ComponentStarter;

use super::StateSyncRunner;

const BUFFER_SIZE: usize = 1000;

#[test]
fn run_returns_when_sync_future_returns() {
    let (_request_sender, request_receiver) = mpsc::channel(BUFFER_SIZE);
    let (storage_reader, _storage_writer) = get_test_storage().0;
    let sync_future = ready(Ok(())).boxed();
    let mut state_sync_runner = StateSyncRunner { request_receiver, storage_reader, sync_future };
    state_sync_runner.start().now_or_never().unwrap().unwrap();
}

#[test]
fn run_returns_error_when_sync_future_returns_error() {
    let (_request_sender, request_receiver) = mpsc::channel(BUFFER_SIZE);
    let (storage_reader, _storage_writer) = get_test_storage().0;
    let sync_future = ready(Err(PapyrusStateSyncError::NoProgress)).boxed();
    let mut state_sync_runner = StateSyncRunner { request_receiver, storage_reader, sync_future };
    state_sync_runner.start().now_or_never().unwrap().unwrap_err();
}
