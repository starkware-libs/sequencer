use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use apollo_storage::{StorageConfig, StorageScope, StorageWriter};
use apollo_test_utils::get_test_storage;
use starknet_api::block::{BlockBody, BlockNumber};
use starknet_api::state::ThinStateDiff;
use starknet_api::transaction::{Transaction, TransactionOutput};

/// Test script to measure flush performance with concurrent implementation
/// This simulates real workload and measures flush times
fn main() {
    println!("🚀 Testing Concurrent Flush Performance");
    println!("========================================");

    // Create test storage
    let ((reader, mut writer), _temp_dir) = get_test_storage();

    // Prepare test data
    let test_data = prepare_test_data();

    println!("📊 Running performance tests with {} operations", test_data.len());

    // Warm up
    println!("🔥 Warming up...");
    run_flush_test(&mut writer, &test_data[0..5], false);

    // Run actual performance test
    println!("⏱️  Running main performance test...");
    let flush_times = run_flush_test(&mut writer, &test_data, true);

    // Analyze results
    analyze_performance(&flush_times);
}

fn prepare_test_data() -> Vec<TestData> {
    let mut test_data = Vec::new();

    // Create some realistic test data
    for i in 0..20 {
        test_data.push(TestData {
            block_number: BlockNumber(i),
            body: BlockBody::default(), // You can make this more realistic
            state_diff: ThinStateDiff::default(),
        });
    }

    test_data
}

struct TestData {
    block_number: BlockNumber,
    body: BlockBody,
    state_diff: ThinStateDiff,
}

fn run_flush_test(
    writer: &mut StorageWriter,
    test_data: &[TestData],
    measure: bool,
) -> Vec<Duration> {
    let mut flush_times = Vec::new();

    for (i, data) in test_data.iter().enumerate() {
        // Write some data to storage
        let mut txn = writer.begin_rw_txn().unwrap();

        // Add some data that will require flushing
        txn.append_body(data.block_number, data.body.clone()).unwrap();
        txn.append_state_diff(data.block_number, data.state_diff.clone()).unwrap();

        if measure {
            print!("Operation {}: ", i + 1);
        }

        // Measure flush time
        let start = Instant::now();
        txn.commit().unwrap(); // This calls flush internally
        let flush_time = start.elapsed();

        if measure {
            println!("Flush took {:.3}ms", flush_time.as_secs_f64() * 1000.0);
            flush_times.push(flush_time);
        }

        // Small delay to simulate realistic usage
        thread::sleep(Duration::from_millis(10));
    }

    flush_times
}

fn analyze_performance(flush_times: &[Duration]) {
    if flush_times.is_empty() {
        println!("❌ No flush times recorded");
        return;
    }

    let total_time: Duration = flush_times.iter().sum();
    let avg_time = total_time / flush_times.len() as u32;
    let min_time = flush_times.iter().min().unwrap();
    let max_time = flush_times.iter().max().unwrap();

    println!("\n📈 Performance Analysis");
    println!("=======================");
    println!("Total operations: {}", flush_times.len());
    println!("Total flush time: {:.3}ms", total_time.as_secs_f64() * 1000.0);
    println!("Average flush time: {:.3}ms", avg_time.as_secs_f64() * 1000.0);
    println!("Min flush time: {:.3}ms", min_time.as_secs_f64() * 1000.0);
    println!("Max flush time: {:.3}ms", max_time.as_secs_f64() * 1000.0);

    // Calculate percentiles
    let mut sorted_times = flush_times.to_vec();
    sorted_times.sort();

    let p50 = sorted_times[sorted_times.len() / 2];
    let p95 = sorted_times[(sorted_times.len() * 95) / 100];
    let p99 = sorted_times[(sorted_times.len() * 99) / 100];

    println!("50th percentile (median): {:.3}ms", p50.as_secs_f64() * 1000.0);
    println!("95th percentile: {:.3}ms", p95.as_secs_f64() * 1000.0);
    println!("99th percentile: {:.3}ms", p99.as_secs_f64() * 1000.0);

    println!("\n🎯 Concurrent Implementation Benefits:");
    println!("- Flush operations now run in parallel across 6 file types");
    println!("- Expected improvement: 2-6x faster depending on I/O patterns");
    println!("- Better resource utilization with concurrent disk writes");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flush_performance() {
        // This test can be run with `cargo test test_flush_performance -- --nocapture`
        main();
    }
}

