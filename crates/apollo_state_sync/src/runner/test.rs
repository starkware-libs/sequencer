use apollo_infra::component_definitions::ComponentStarter;
use apollo_network::network_manager::NetworkError;
use apollo_p2p_sync::client::P2pSyncClientError;
use futures::future::{pending, ready};
use futures::FutureExt;

use super::StateSyncRunner;

#[test]
#[should_panic]
fn run_panics_when_network_future_returns() {
    let network_future = ready(Ok(())).boxed();
    let p2p_sync_client_future = pending().boxed();
    let p2p_sync_server_future = pending().boxed();
    let central_sync_client_future = pending().boxed();
    let new_block_dev_null_future = pending().boxed();
    let rpc_server_future = pending().boxed();
    let register_metrics_future = pending().boxed();
    let mut state_sync_runner = StateSyncRunner {
        network_future,
        p2p_sync_client_future,
        p2p_sync_server_future,
        central_sync_client_future,
        new_block_dev_null_future,
        rpc_server_future,
        register_metrics_future,
    };
    state_sync_runner.start().now_or_never().unwrap();
}

#[test]
#[should_panic]
fn run_panics_when_network_future_returns_error() {
    let network_future =
        ready(Err(NetworkError::DialError(libp2p::swarm::DialError::Aborted))).boxed();
    let p2p_sync_client_future = pending().boxed();
    let p2p_sync_server_future = pending().boxed();
    let central_sync_client_future = pending().boxed();
    let new_block_dev_null_future = pending().boxed();
    let rpc_server_future = pending().boxed();
    let register_metrics_future = pending().boxed();
    let mut state_sync_runner = StateSyncRunner {
        network_future,
        p2p_sync_client_future,
        p2p_sync_server_future,
        central_sync_client_future,
        new_block_dev_null_future,
        rpc_server_future,
        register_metrics_future,
    };
    state_sync_runner.start().now_or_never().unwrap();
}

#[test]
#[should_panic]
fn run_panics_when_sync_client_future_returns_error() {
    let network_future = pending().boxed();
    let p2p_sync_client_future = ready(Err(P2pSyncClientError::TooManyResponses)).boxed();
    let p2p_sync_server_future = pending().boxed();
    let central_sync_client_future = pending().boxed();
    let new_block_dev_null_future = pending().boxed();
    let rpc_server_future = pending().boxed();
    let register_metrics_future = pending().boxed();
    let mut state_sync_runner = StateSyncRunner {
        network_future,
        p2p_sync_client_future,
        p2p_sync_server_future,
        central_sync_client_future,
        new_block_dev_null_future,
        rpc_server_future,
        register_metrics_future,
    };
    state_sync_runner.start().now_or_never().unwrap();
}
