#[cfg(test)]
mod concurrent_flush_performance_test {
    use std::time::Instant;

    use starknet_api::block::{BlockBody, BlockNumber};
    use starknet_api::state::ThinStateDiff;

    use crate::body::BodyStorageWriter;
    use crate::state::StateStorageWriter;
    use crate::test_utils::get_test_storage;
    use crate::{FileHandlers, RW};

    #[test]
    fn test_concurrent_flush_performance() {
        let ((_reader, mut writer), _temp_dir) = get_test_storage();
        // Warm up phase (to avoid measurements to be affected by initial slowness and not
        // representative of real performance).
        for i in 0..5 {
            let txn = writer.begin_rw_txn().unwrap();
            let txn = txn.append_body(BlockNumber(i), BlockBody::default()).unwrap();
            let txn = txn.append_state_diff(BlockNumber(i), ThinStateDiff::default()).unwrap();
            txn.commit().unwrap();
        }

        // Main performance test with 15 operations.
        let mut flush_times = Vec::new();
        for i in 0..15 {
            let txn = writer.begin_rw_txn().unwrap();
            let txn = txn.append_body(BlockNumber(i + 5), BlockBody::default()).unwrap();
            let txn = txn.append_state_diff(BlockNumber(i + 5), ThinStateDiff::default()).unwrap();
            let start = Instant::now();
            txn.commit().unwrap();
            let elapsed = start.elapsed();
            flush_times.push(elapsed);
            println!("Operation {}: Flush took {:.3}ms", i + 1, elapsed.as_secs_f64() * 1000.0);

            // Small delay to simulate realistic usage.
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        let total_time: std::time::Duration = flush_times.iter().sum();
        let avg_time = total_time / flush_times.len() as u32;
        let min_time = flush_times.iter().min().unwrap();
        let max_time = flush_times.iter().max().unwrap();

        println!("Total operations: {}", flush_times.len());
        println!("Total flush time: {:.3}ms", total_time.as_secs_f64() * 1000.0);
        println!("Average flush time: {:.3}ms", avg_time.as_secs_f64() * 1000.0);
        println!("Min flush time: {:.3}ms", min_time.as_secs_f64() * 1000.0);
        println!("Max flush time: {:.3}ms", max_time.as_secs_f64() * 1000.0);

        let mut sorted_times = flush_times.clone();
        sorted_times.sort();

        let p50 = sorted_times[sorted_times.len() / 2];
        let p95 = sorted_times[(sorted_times.len() * 95) / 100];
        let p99 = sorted_times[(sorted_times.len() * 99) / 100];

        println!("50th percentile (median): {:.3}ms", p50.as_secs_f64() * 1000.0);
        println!("95th percentile: {:.3}ms", p95.as_secs_f64() * 1000.0);
        println!("99th percentile: {:.3}ms", p99.as_secs_f64() * 1000.0);

        assert!(!flush_times.is_empty(), "Should have collected flush timing data");
        assert!(avg_time.as_secs() < 1, "Average flush time should be reasonable (< 1 second)");
    }

    #[test]
    fn test_sequential_vs_concurrent_flush_comparison() {
        println!("Testing SEQUENTIAL flush");
        let sequential_times = run_flush_test_with_method("Sequential", |file_handlers| {
            file_handlers.flush();
        });
        println!("Testing CONCURRENT flush");
        let concurrent_times = run_flush_test_with_method("Concurrent", |file_handlers| {
            file_handlers.flush_concurrent();
        });
        compare_performance(sequential_times, concurrent_times);
    }

    fn run_flush_test_with_method<F>(method_name: &str, flush_method: F) -> Vec<std::time::Duration>
    where
        F: Fn(&FileHandlers<RW>),
    {
        let ((_reader, mut writer), _temp_dir) = get_test_storage();
        let mut flush_times = Vec::new();
        for i in 0..3 {
            let txn = writer.begin_rw_txn().unwrap();
            let txn = txn.append_body(BlockNumber(i), BlockBody::default()).unwrap();
            let txn = txn.append_state_diff(BlockNumber(i), ThinStateDiff::default()).unwrap();
            let start = Instant::now();
            flush_method(&txn.file_handlers);
            let elapsed = start.elapsed();
            txn.commit().unwrap();

            println!("  {} warmup {}: {:.3}ms", method_name, i + 1, elapsed.as_secs_f64() * 1000.0);
        }

        for i in 0..10 {
            let txn = writer.begin_rw_txn().unwrap();
            let txn = txn.append_body(BlockNumber(i + 3), BlockBody::default()).unwrap();
            let txn = txn.append_state_diff(BlockNumber(i + 3), ThinStateDiff::default()).unwrap();
            let start = Instant::now();
            flush_method(&txn.file_handlers);
            let elapsed = start.elapsed();
            txn.commit().unwrap();
            flush_times.push(elapsed);
            println!(
                "  {} operation {}: {:.3}ms",
                method_name,
                i + 1,
                elapsed.as_secs_f64() * 1000.0
            );

            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        flush_times
    }

    fn compare_performance(
        sequential_times: Vec<std::time::Duration>,
        concurrent_times: Vec<std::time::Duration>,
    ) {
        let seq_avg =
            sequential_times.iter().sum::<std::time::Duration>() / sequential_times.len() as u32;
        let conc_avg =
            concurrent_times.iter().sum::<std::time::Duration>() / concurrent_times.len() as u32;
        let seq_min = sequential_times.iter().min().unwrap();
        let seq_max = sequential_times.iter().max().unwrap();
        let conc_min = concurrent_times.iter().min().unwrap();
        let conc_max = concurrent_times.iter().max().unwrap();
        println!("SEQUENTIAL:");
        println!("   Average: {:.3}ms", seq_avg.as_secs_f64() * 1000.0);
        println!("   Min:     {:.3}ms", seq_min.as_secs_f64() * 1000.0);
        println!("   Max:     {:.3}ms", seq_max.as_secs_f64() * 1000.0);
        println!("CONCURRENT:");
        println!("   Average: {:.3}ms", conc_avg.as_secs_f64() * 1000.0);
        println!("   Min:     {:.3}ms", conc_min.as_secs_f64() * 1000.0);
        println!("   Max:     {:.3}ms", conc_max.as_secs_f64() * 1000.0);
        let improvement_factor = seq_avg.as_secs_f64() / conc_avg.as_secs_f64();
        let time_saved = seq_avg.as_secs_f64() - conc_avg.as_secs_f64();
        println!("IMPROVEMENT ANALYSIS:");
        println!("Speedup factor: {:.2}x faster", improvement_factor);
        println!("Time saved per flush: {:.3}ms", time_saved * 1000.0);
        println!("Performance improvement: {:.1}%", (improvement_factor - 1.0) * 100.0);
    }
}
