//! Tests for the admission/queue reject paths in [`ProvingRpcServerImpl`].
//!
//! Sizing the semaphores so the reject fires before the prover runs covers both busy-reject
//! outcomes without a live node or a real proving run. Zero admission capacity forces a
//! queue-full reject. Zero worker slots with a tiny wait timeout force a wait-timeout reject.

use std::time::Duration;

use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use blockifier_test_utils::calldata::create_calldata;
use starknet_api::core::ContractAddress;
use starknet_api::rpc_transaction::RpcTransaction;

use crate::config::ProverConfig;
use crate::proving::virtual_snos_prover::RpcVirtualSnosProver;
use crate::server::metrics::{names as metric_names, outcomes};
use crate::server::rpc_api::ProvingRpcServer;
use crate::server::rpc_impl::{ProvingRpcServerImpl, SaturationClearGuard};
use crate::server::saturation::SaturationMonitor;
use crate::server::test_recorder::{metric_value, outcome_total_line, shared_handle};
use crate::test_utils::{build_client_side_rpc_invoke, DUMMY_ACCOUNT_ADDRESS};

/// JSON-RPC error code returned by `service_busy` (see `server::errors`).
const SERVICE_BUSY_CODE: i32 = -32005;

fn dummy_prover() -> RpcVirtualSnosProver {
    let config =
        ProverConfig { rpc_node_url: "http://localhost:1".to_string(), ..Default::default() };
    RpcVirtualSnosProver::new(&config)
}

/// The reject fires at admission/wait, before the transaction is inspected, so any request works.
fn dummy_request() -> RpcTransaction {
    let account = ContractAddress::try_from(DUMMY_ACCOUNT_ADDRESS).unwrap();
    build_client_side_rpc_invoke(account, create_calldata(account, "noop", &[]))
}

#[tokio::test]
async fn full_queue_rejects_with_service_busy_and_counts_queue_full() {
    let handle = shared_handle();
    let line = outcome_total_line(outcomes::REJECTED_QUEUE_FULL);
    let before = metric_value(&handle.render(), &line);

    // max_concurrent + max_queued = 0 gives an admission capacity of 0, so admission rejects
    // every request.
    let saturation_monitor = SaturationMonitor::default();
    let rpc_impl = ProvingRpcServerImpl::new(
        dummy_prover(),
        0,
        0,
        Duration::from_secs(30),
        saturation_monitor.clone(),
    );
    let error = rpc_impl
        .prove_transaction(BlockId::Latest, dummy_request())
        .await
        .expect_err("a full queue must reject");

    assert_eq!(error.code(), SERVICE_BUSY_CODE);
    assert_eq!(metric_value(&handle.render(), &line) - before, 1.0, "rejected_queue_full delta");
    assert!(
        saturation_monitor.saturated_for_at_least(Duration::ZERO),
        "a queue-full reject must open the saturation window"
    );
}

#[tokio::test]
async fn wait_timeout_rejects_with_service_busy_and_counts_wait_timeout() {
    let handle = shared_handle();
    let line = outcome_total_line(outcomes::REJECTED_WAIT_TIMEOUT);
    let before = metric_value(&handle.render(), &line);
    let gauge_before = metric_value(&handle.render(), metric_names::QUEUE_WAITING_REQUESTS);

    // One queue slot but zero worker slots, with a tiny backstop timeout. Admission lets the
    // request in, it waits for a worker that never frees, and the timeout rejects it.
    let saturation_monitor = SaturationMonitor::default();
    let rpc_impl = ProvingRpcServerImpl::new(
        dummy_prover(),
        0,
        1,
        Duration::from_millis(10),
        saturation_monitor.clone(),
    );
    let error = rpc_impl
        .prove_transaction(BlockId::Latest, dummy_request())
        .await
        .expect_err("a wait-timeout must reject");

    assert_eq!(error.code(), SERVICE_BUSY_CODE);
    assert_eq!(metric_value(&handle.render(), &line) - before, 1.0, "rejected_wait_timeout delta");
    // The queue-depth guard ran on the timeout path, so the gauge returns to its prior value.
    assert_eq!(
        metric_value(&handle.render(), metric_names::QUEUE_WAITING_REQUESTS),
        gauge_before,
        "queue-depth gauge returned to baseline",
    );
    assert!(
        saturation_monitor.saturated_for_at_least(Duration::ZERO),
        "a wait-timeout reject must open the saturation window"
    );
}

/// Saturation must clear when an in-flight job releases its worker slot, even if no further
/// request arrives. This test opens the window directly instead of going through a
/// `prove_transaction` reject, whose extra `rejected_*` outcome would race the exact-delta
/// assertions of the two reject tests above on the shared metrics recorder.
#[test]
fn saturation_clear_guard_drop_clears_saturation_without_new_traffic() {
    let saturation_monitor = SaturationMonitor::default();
    let in_flight_release_guard =
        SaturationClearGuard { saturation_monitor: saturation_monitor.clone() };

    // Rejects open the window while the in-flight job holds the only worker slot.
    saturation_monitor.mark_rejected();
    assert!(saturation_monitor.saturated_for_at_least(Duration::ZERO));

    // The in-flight job finishes and its guard drops, with no new request arriving.
    drop(in_flight_release_guard);
    assert!(
        !saturation_monitor.saturated_for_at_least(Duration::ZERO),
        "releasing the worker slot must clear the saturation window"
    );
}

/// A request that wins a worker slot must clear the saturation window at that moment, not only
/// when the slot is later released. This is the only test that fails if the `mark_progress` call
/// on acquisition goes away and clearing is left to `SaturationClearGuard`'s drop. The assertion
/// runs while both the guard and the permit are still held, so only the acquisition can have
/// cleared the window.
#[tokio::test]
async fn accepted_request_clears_saturation_window_while_in_flight() {
    let saturation_monitor = SaturationMonitor::default();
    let rpc_impl = ProvingRpcServerImpl::new(
        dummy_prover(),
        1,
        0,
        Duration::from_secs(30),
        saturation_monitor.clone(),
    );

    // Open the window, as a burst of busy-rejects would.
    saturation_monitor.mark_rejected();
    assert!(saturation_monitor.saturated_for_at_least(Duration::ZERO));

    let _worker_slot =
        rpc_impl.acquire_worker_slot().await.expect("the only worker slot is free, so it is won");
    assert!(
        !saturation_monitor.saturated_for_at_least(Duration::ZERO),
        "a request holding a worker slot must have cleared the saturation window"
    );
}
