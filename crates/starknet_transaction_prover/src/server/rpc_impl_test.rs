use std::time::Duration;

use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use starknet_api::invoke_tx_args;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::test_utils::invoke::rpc_invoke_tx;
use tracing_test::traced_test;

use crate::config::ProverConfig;
use crate::proving::virtual_snos_prover::RpcVirtualSnosProver;
use crate::server::errors::ServiceErrorCode;
use crate::server::rpc_api::ProvingRpcServer;
use crate::server::rpc_impl::ProvingRpcServerImpl;

/// Nothing listens here: every test below is rejected before the prover is called, so a request
/// that slipped through fails loudly instead of quietly passing.
const UNBOUND_RPC_NODE_URL: &str = "http://localhost:1";

fn server(max_concurrent_requests: usize, max_queued_requests: usize) -> ProvingRpcServerImpl {
    let config =
        ProverConfig { rpc_node_url: UNBOUND_RPC_NODE_URL.to_string(), ..Default::default() };
    ProvingRpcServerImpl::new(
        RpcVirtualSnosProver::new(&config),
        max_concurrent_requests,
        max_queued_requests,
        Duration::from_millis(1),
    )
}

/// No admission permits at all, so a request is turned away without ever queueing.
fn server_with_full_queue() -> ProvingRpcServerImpl {
    server(0, 0)
}

/// One queue slot but no worker slots, so a request is admitted and then waits forever.
fn server_with_no_free_worker() -> ProvingRpcServerImpl {
    server(0, 1)
}

fn any_transaction() -> RpcTransaction {
    rpc_invoke_tx(invoke_tx_args!())
}

/// With no admission permits the request is rejected outright, without waiting. Both rejection
/// paths return the same code, so the log line is what distinguishes them - without it, deleting
/// the admission check would leave this test passing via the queue-wait timeout instead.
#[traced_test]
#[tokio::test]
async fn rejects_with_service_busy_when_the_queue_is_full() {
    let server = server_with_full_queue();

    let error = server
        .prove_transaction(BlockId::Latest, any_transaction())
        .await
        .expect_err("a full queue must reject");

    assert_eq!(error.code(), ServiceErrorCode::ServiceBusy.code());
    assert!(logs_contain("queue is full"));
    assert!(!logs_contain("timed out waiting for a worker slot"));
}

/// Admitted into the queue, but no worker slot ever frees up, so the queue-wait backstop fires.
#[traced_test]
#[tokio::test]
async fn rejects_with_service_busy_when_waiting_for_a_worker_times_out() {
    let server = server_with_no_free_worker();

    let error = server
        .prove_transaction(BlockId::Latest, any_transaction())
        .await
        .expect_err("an expired queue wait must reject");

    assert_eq!(error.code(), ServiceErrorCode::ServiceBusy.code());
    assert!(logs_contain("timed out waiting for a worker slot"));
    assert!(!logs_contain("queue is full"));
}

/// `spec_version` must stay outside admission control: it is what a load balancer or the smoke
/// script probes, and gating it behind proving capacity would make a busy service look down.
#[tokio::test]
async fn spec_version_is_served_even_when_the_queue_is_full() {
    let server = server_with_full_queue();

    server.spec_version().await.expect("spec_version must not be gated by proving capacity");
}
