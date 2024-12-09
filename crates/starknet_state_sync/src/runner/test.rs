use futures::channel::mpsc;
use futures::future::{pending, ready};
use futures::FutureExt;
use papyrus_network::network_manager::NetworkError;
use papyrus_p2p_sync::client::P2PSyncClientError;
use papyrus_storage::test_utils::get_test_storage;
use starknet_sequencer_infra::component_definitions::ComponentStarter;

use super::StateSyncRunner;

const BUFFER_SIZE: usize = 1000;

#[test]
fn run_returns_when_network_future_returns() {
    let (_request_sender, request_receiver) = mpsc::channel(BUFFER_SIZE);
    let (storage_reader, _storage_writer) = get_test_storage().0;
    let network_future = ready(Ok(())).boxed();
    let p2p_sync_client_future = pending().boxed();
    let p2p_sync_server_future = pending().boxed();
    let mut state_sync_runner = StateSyncRunner {
        request_receiver,
        storage_reader,
        network_future,
        p2p_sync_client_future,
        p2p_sync_server_future,
    };
    state_sync_runner.start().now_or_never().unwrap().unwrap();
}

#[test]
fn run_returns_when_sync_client_future_returns() {
    let (_request_sender, request_receiver) = mpsc::channel(BUFFER_SIZE);
    let (storage_reader, _storage_writer) = get_test_storage().0;
    let network_future = pending().boxed();
    let p2p_sync_client_future = ready(Ok(())).boxed();
    let p2p_sync_server_future = pending().boxed();
    let mut state_sync_runner = StateSyncRunner {
        request_receiver,
        storage_reader,
        network_future,
        p2p_sync_client_future,
        p2p_sync_server_future,
    };
    state_sync_runner.start().now_or_never().unwrap().unwrap();
}

#[test]
fn run_returns_error_when_sync_server_future_returns() {
    let (_request_sender, request_receiver) = mpsc::channel(BUFFER_SIZE);
    let (storage_reader, _storage_writer) = get_test_storage().0;
    let network_future = pending().boxed();
    let p2p_sync_client_future = pending().boxed();
    let p2p_sync_server_future = ready(()).boxed();
    let mut state_sync_runner = StateSyncRunner {
        request_receiver,
        storage_reader,
        network_future,
        p2p_sync_client_future,
        p2p_sync_server_future,
    };
    state_sync_runner.start().now_or_never().unwrap().unwrap_err();
}

#[test]
fn run_returns_error_when_network_future_returns_error() {
    let (_request_sender, request_receiver) = mpsc::channel(BUFFER_SIZE);
    let (storage_reader, _storage_writer) = get_test_storage().0;
    let network_future =
        ready(Err(NetworkError::DialError(libp2p::swarm::DialError::Aborted))).boxed();
    let p2p_sync_client_future = pending().boxed();
    let p2p_sync_server_future = pending().boxed();
    let mut state_sync_runner = StateSyncRunner {
        request_receiver,
        storage_reader,
        network_future,
        p2p_sync_client_future,
        p2p_sync_server_future,
    };
    state_sync_runner.start().now_or_never().unwrap().unwrap_err();
}

#[test]
fn run_returns_error_when_sync_client_future_returns_error() {
    let (_request_sender, request_receiver) = mpsc::channel(BUFFER_SIZE);
    let (storage_reader, _storage_writer) = get_test_storage().0;
    let network_future = pending().boxed();
    let p2p_sync_client_future = ready(Err(P2PSyncClientError::TooManyResponses)).boxed();
    let p2p_sync_server_future = pending().boxed();
    let mut state_sync_runner = StateSyncRunner {
        request_receiver,
        storage_reader,
        network_future,
        p2p_sync_client_future,
        p2p_sync_server_future,
    };
    state_sync_runner.start().now_or_never().unwrap().unwrap_err();
}
