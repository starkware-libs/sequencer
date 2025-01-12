use futures::future::{pending, ready};
use futures::FutureExt;
use papyrus_network::network_manager::NetworkError;
use papyrus_p2p_sync::client::P2pSyncClientError;
use starknet_sequencer_infra::component_definitions::ComponentStarter;

use super::StateSyncRunner;

#[test]
fn run_returns_when_network_future_returns() {
    let network_future = ready(Ok(())).boxed();
    let p2p_sync_client_future = pending().boxed();
    let p2p_sync_server_future = pending().boxed();
    let mut state_sync_runner =
        StateSyncRunner { network_future, p2p_sync_client_future, p2p_sync_server_future };
    state_sync_runner.start().now_or_never().unwrap().unwrap();
}

#[test]
fn run_returns_error_when_network_future_returns_error() {
    let network_future =
        ready(Err(NetworkError::DialError(libp2p::swarm::DialError::Aborted))).boxed();
    let p2p_sync_client_future = pending().boxed();
    let p2p_sync_server_future = pending().boxed();
    let mut state_sync_runner =
        StateSyncRunner { network_future, p2p_sync_client_future, p2p_sync_server_future };
    state_sync_runner.start().now_or_never().unwrap().unwrap_err();
}

#[test]
fn run_returns_error_when_sync_client_future_returns_error() {
    let network_future = pending().boxed();
    let p2p_sync_client_future = ready(Err(P2pSyncClientError::TooManyResponses)).boxed();
    let p2p_sync_server_future = pending().boxed();
    let mut state_sync_runner =
        StateSyncRunner { network_future, p2p_sync_client_future, p2p_sync_server_future };
    state_sync_runner.start().now_or_never().unwrap().unwrap_err();
}
