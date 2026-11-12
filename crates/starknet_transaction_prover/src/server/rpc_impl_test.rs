//! Tests for [`ProvingRpcServerImpl`]: the admission/queue reject paths and the failure logging.
//!
//! Sizing the semaphores so the reject fires before the prover runs covers both busy-reject
//! outcomes without a live node or a real proving run. Zero admission capacity forces a
//! queue-full reject. Zero worker slots with a tiny wait timeout force a wait-timeout reject.
//! Input validation also rejects before any runner I/O, which covers the failure logs.
//!
//! Each test that asserts on metrics installs its own local Prometheus recorder, so its samples
//! are exact and independent of the other tests in the binary. A local recorder is thread-local,
//! so those tests run on a current-thread runtime and hold the recorder guard across the awaited
//! call and the scrape.

use std::time::Duration;

use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use blockifier_test_utils::calldata::create_calldata;
use starknet_api::block::GasPrice;
use starknet_api::core::ContractAddress;
use starknet_api::rpc_transaction::{RpcInvokeTransaction, RpcTransaction};
use tracing_test::traced_test;

use crate::config::ProverConfig;
use crate::proving::virtual_snos_prover::RpcVirtualSnosProver;
use crate::server::metrics::{configured_builder, names as metric_names, outcomes};
use crate::server::rpc_api::ProvingRpcServer;
use crate::server::rpc_impl::{ProvingRpcServerImpl, WorkerSlotGuard};
use crate::server::saturation::SaturationMonitor;
use crate::server::test_recorder::{metric_value, outcome_total_line};
use crate::test_utils::{build_client_side_rpc_invoke, DUMMY_ACCOUNT_ADDRESS};

/// JSON-RPC error code returned by `service_busy` (see `server::errors`).
const SERVICE_BUSY_CODE: i32 = -32005;
/// JSON-RPC error code returned by `invalid_transaction_input` (see `server::errors`).
const INVALID_TRANSACTION_INPUT_CODE: i32 = 1000;

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

#[tokio::test(flavor = "current_thread")]
async fn full_queue_rejects_with_service_busy_and_counts_queue_full() {
    let recorder = configured_builder().unwrap().build_recorder();
    let handle = recorder.handle();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);

    // max_concurrent + max_queued = 0 gives an admission capacity of 0, so admission rejects
    // every request.
    let saturation_monitor = SaturationMonitor::default();
    // A full queue means proving runs hold the worker slots. Stand one in so the open window
    // reports as saturation.
    let _in_flight_slot = WorkerSlotGuard::new(saturation_monitor.clone());
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
    assert_eq!(
        metric_value(&handle.render(), &outcome_total_line(outcomes::REJECTED_QUEUE_FULL)),
        1.0,
        "rejected_queue_full"
    );
    assert!(
        saturation_monitor.saturated_for_at_least(Duration::ZERO),
        "a queue-full reject must open the saturation window"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn wait_timeout_rejects_with_service_busy_and_counts_wait_timeout() {
    let recorder = configured_builder().unwrap().build_recorder();
    let handle = recorder.handle();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);

    // One queue slot but zero worker slots, with a tiny backstop timeout. Admission lets the
    // request in, it waits for a worker that never frees, and the timeout rejects it.
    let saturation_monitor = SaturationMonitor::default();
    // Stand in for the proving run that keeps the worker busy.
    let _in_flight_slot = WorkerSlotGuard::new(saturation_monitor.clone());
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
    let scrape = handle.render();
    assert_eq!(
        metric_value(&scrape, &outcome_total_line(outcomes::REJECTED_WAIT_TIMEOUT)),
        1.0,
        "rejected_wait_timeout"
    );
    // The queue-depth guard ran on the timeout path, so the gauge is back at zero.
    assert_eq!(
        metric_value(&scrape, metric_names::QUEUE_WAITING_REQUESTS),
        0.0,
        "queue-depth gauge returned to zero",
    );
    assert!(
        saturation_monitor.saturated_for_at_least(Duration::ZERO),
        "a wait-timeout reject must open the saturation window"
    );
}

/// A failed request logs the origin event naming the step that failed. It logs the outcome in
/// `prove_transaction completed` and again in `prove_transaction_failed`. None of these records
/// may carry the client's transaction data, so the distinctive fee price that trips validation
/// must not appear anywhere in the captured logs.
#[tokio::test(flavor = "current_thread")]
#[traced_test]
async fn rejected_fee_input_logs_origin_and_outcome_without_the_fee_value() {
    const DISTINCTIVE_FEE_PRICE: u128 = 7_654_321;
    let rpc_impl = ProvingRpcServerImpl::new(
        dummy_prover(),
        1,
        0,
        Duration::from_secs(30),
        SaturationMonitor::default(),
    );
    let mut request = dummy_request();
    let RpcTransaction::Invoke(RpcInvokeTransaction::V3(invoke)) = &mut request else {
        unreachable!("dummy_request builds an invoke V3 transaction")
    };
    invoke.resource_bounds.l2_gas.max_price_per_unit = GasPrice(DISTINCTIVE_FEE_PRICE);

    let error = rpc_impl
        .prove_transaction(BlockId::Latest, request)
        .await
        .expect_err("a nonzero fee price must fail validation");

    assert_eq!(error.code(), INVALID_TRANSACTION_INPUT_CODE);
    assert!(logs_contain("event=\"validation_error\""), "origin event");
    assert!(logs_contain("reason=\"invalid_transaction_input\""), "origin reason");
    assert!(logs_contain("event=\"prove_transaction_failed\""), "final outcome event");
    assert!(logs_contain("outcome=\"failure_validation\""), "final outcome label");
    assert!(
        !logs_contain(&DISTINCTIVE_FEE_PRICE.to_string()),
        "the client's fee price must not reach the log stream"
    );
}

/// Saturation must clear when an in-flight job releases its worker slot, even if no further
/// request arrives. The guard alone is under test, so the window is opened directly. A second job
/// keeps its slot, so only the release can have cleared the window.
#[test]
fn worker_slot_release_clears_saturation_without_new_traffic() {
    let saturation_monitor = SaturationMonitor::default();
    let finishing_slot = WorkerSlotGuard::new(saturation_monitor.clone());
    let _still_running_slot = WorkerSlotGuard::new(saturation_monitor.clone());

    // Rejects open the window while both in-flight jobs hold worker slots.
    saturation_monitor.mark_rejected();
    assert!(saturation_monitor.saturated_for_at_least(Duration::ZERO));

    // One job finishes and its guard drops, with no new request arriving.
    drop(finishing_slot);
    assert!(
        !saturation_monitor.saturated_for_at_least(Duration::ZERO),
        "releasing a worker slot must clear the saturation window"
    );
}

/// A request that wins a worker slot must clear the saturation window at that moment, not only
/// when the slot is later released. It fails if `WorkerSlotGuard::new` stops closing the window
/// and clearing is left to the guard's drop. The assertion runs while both the guard and the
/// permit are still held, so only the acquisition can have cleared the window.
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

    // Open the window, as a burst of busy-rejects would while another job holds a slot.
    let _other_job_slot = WorkerSlotGuard::new(saturation_monitor.clone());
    saturation_monitor.mark_rejected();
    assert!(saturation_monitor.saturated_for_at_least(Duration::ZERO));

    let _worker_slot =
        rpc_impl.acquire_worker_slot().await.expect("the only worker slot is free, so it is won");
    assert!(
        !saturation_monitor.saturated_for_at_least(Duration::ZERO),
        "a request holding a worker slot must have cleared the saturation window"
    );
}
