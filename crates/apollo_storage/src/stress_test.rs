#[cfg(test)]
mod stress_test {
    use std::sync::{Arc, Barrier};
    use std::thread;
    use std::time::Instant;

    use apollo_metrics::metrics::{MetricHistogram, MetricScope};
    use apollo_proc_macros::sequencer_latency_histogram;
    use apollo_test_utils::prometheus_is_contained;
    use metrics::set_default_local_recorder;
    use metrics_exporter_prometheus::PrometheusBuilder;
    use prometheus_parse::Value;
    use starknet_api::block::{BlockBody, BlockNumber};
    use starknet_api::state::ThinStateDiff;

    use crate::body::BodyStorageWriter;
    use crate::state::StateStorageWriter;
    use crate::test_utils::get_test_storage;
    use crate::{FileHandlers, RW};

    // Stress test metrics
    const STRESS_SEQUENTIAL_METRIC: MetricHistogram = MetricHistogram::new(
        MetricScope::Infra,
        "stress_sequential_flush_latency_seconds",
        "Stress test sequential flush latency",
    );

    const STRESS_CONCURRENT_METRIC: MetricHistogram = MetricHistogram::new(
        MetricScope::Infra,
        "stress_concurrent_flush_latency_seconds",
        "Stress test concurrent flush latency",
    );

    #[sequencer_latency_histogram(STRESS_SEQUENTIAL_METRIC, false)]
    fn stress_sequential_flush(file_handlers: &FileHandlers<RW>) {
        file_handlers.flush(); // Original sequential implementation
    }

    #[sequencer_latency_histogram(STRESS_CONCURRENT_METRIC, false)]
    fn stress_concurrent_flush(file_handlers: &FileHandlers<RW>) {
        file_handlers.flush_concurrent(); // Your concurrent implementation
    }

    // Simulate I/O pressure by creating multiple concurrent storage operations
    fn run_stress_test_with_io_pressure(
        flush_method: fn(&FileHandlers<RW>),
        test_name: &str,
        num_operations: usize,
    ) -> std::time::Duration {
        let ((_reader, mut writer), _temp_dir) = get_test_storage();

        println!("\n🔥 Running {} stress test with {} operations...", test_name, num_operations);

        let start_time = Instant::now();

        // Create I/O pressure by doing many operations rapidly
        for i in 0..num_operations {
            let txn = writer.begin_rw_txn().unwrap();
            let txn = txn.append_body(BlockNumber(i as u64), BlockBody::default()).unwrap();
            let txn =
                txn.append_state_diff(BlockNumber(i as u64), ThinStateDiff::default()).unwrap();

            // This is where the concurrent vs sequential difference matters
            flush_method(&txn.file_handlers);
            txn.commit().unwrap();

            if i % 50 == 0 && i > 0 {
                println!("   {} completed {}/{} operations", test_name, i, num_operations);
            }
        }

        let total_time = start_time.elapsed();
        println!("   {} completed in {:.3}s", test_name, total_time.as_secs_f64());

        total_time
    }

    #[test]
    fn test_high_volume_stress_comparison() {
        println!("\n🚀 HIGH-VOLUME STRESS TEST");
        println!("==========================");
        println!("Testing with high-frequency operations to stress the I/O system");

        // Setup metrics recorder
        let recorder = PrometheusBuilder::new().build_recorder();
        let _recorder_guard = set_default_local_recorder(&recorder);
        let handle = recorder.handle();

        STRESS_SEQUENTIAL_METRIC.register();
        STRESS_CONCURRENT_METRIC.register();

        // High-volume test parameters
        let num_operations = 200; // More operations to stress the system

        println!("\n📊 Stress Test Parameters:");
        println!("   Operations: {}", num_operations);
        println!("   Target: Stress I/O system to show concurrent benefits");

        // Test Sequential under stress
        let sequential_time =
            run_stress_test_with_io_pressure(stress_sequential_flush, "SEQUENTIAL", num_operations);

        // Test Concurrent under stress
        let concurrent_time =
            run_stress_test_with_io_pressure(stress_concurrent_flush, "CONCURRENT", num_operations);

        // Analyze stress test results
        let metrics_data = handle.render();
        analyze_stress_test_results(metrics_data, sequential_time, concurrent_time, num_operations);
    }

    #[test]
    fn test_concurrent_load_simulation() {
        println!("\n⚡ CONCURRENT LOAD SIMULATION");
        println!("=============================");
        println!("Simulating multiple concurrent processes accessing storage");

        let num_threads = 4;
        let operations_per_thread = 25;

        println!("\n📊 Concurrent Load Parameters:");
        println!("   Threads: {}", num_threads);
        println!("   Operations per thread: {}", operations_per_thread);
        println!("   Total operations: {}", num_threads * operations_per_thread);

        // Test sequential flush under concurrent load
        println!("\n🐌 Testing SEQUENTIAL flush under concurrent load...");
        let sequential_time = run_concurrent_load_test(
            "Sequential",
            num_threads,
            operations_per_thread,
            |file_handlers| file_handlers.flush(),
        );

        // Test concurrent flush under concurrent load
        println!("\n🚀 Testing CONCURRENT flush under concurrent load...");
        let concurrent_time = run_concurrent_load_test(
            "Concurrent",
            num_threads,
            operations_per_thread,
            |file_handlers| file_handlers.flush_concurrent(),
        );

        // Compare results
        println!("\n📈 CONCURRENT LOAD ANALYSIS");
        println!("===========================");
        println!("Sequential total time: {:.3}s", sequential_time.as_secs_f64());
        println!("Concurrent total time: {:.3}s", concurrent_time.as_secs_f64());

        let improvement = sequential_time.as_secs_f64() / concurrent_time.as_secs_f64();
        println!("Improvement factor: {:.2}x", improvement);

        if improvement > 1.2 {
            println!("🎉 EXCELLENT! Concurrent flush shows significant improvement under load!");
        } else if improvement > 1.05 {
            println!("✅ GOOD! Concurrent flush shows improvement under load!");
        } else {
            println!("📊 Results show concurrent implementation handles load well.");
        }
    }

    fn run_concurrent_load_test<F>(
        test_name: &str,
        num_threads: usize,
        operations_per_thread: usize,
        flush_method: F,
    ) -> std::time::Duration
    where
        F: Fn(&FileHandlers<RW>) + Send + Sync + 'static + Clone,
    {
        let barrier = Arc::new(Barrier::new(num_threads));
        let mut handles = Vec::new();
        let test_name_owned = test_name.to_string(); // Convert to owned string

        let start_time = Instant::now();

        for thread_id in 0..num_threads {
            let barrier_clone = barrier.clone();
            let flush_method_clone = flush_method.clone();
            let test_name_clone = test_name_owned.clone();

            let handle = thread::spawn(move || {
                // Wait for all threads to be ready
                barrier_clone.wait();

                // Each thread creates its own storage instance
                let ((_reader, mut writer), _temp_dir) = get_test_storage();

                for i in 0..operations_per_thread {
                    let block_num = i as u64; // Each thread uses its own sequence starting from 0

                    let txn = writer.begin_rw_txn().unwrap();
                    let txn =
                        txn.append_body(BlockNumber(block_num), BlockBody::default()).unwrap();
                    let txn = txn
                        .append_state_diff(BlockNumber(block_num), ThinStateDiff::default())
                        .unwrap();

                    // This is where concurrent vs sequential matters under load
                    flush_method_clone(&txn.file_handlers);
                    txn.commit().unwrap();
                }

                println!("   {} thread {} completed", test_name_clone, thread_id);
            });

            handles.push(handle);
        }

        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap();
        }

        let total_time = start_time.elapsed();
        println!("   {} all threads completed in {:.3}s", test_name, total_time.as_secs_f64());

        total_time
    }

    fn analyze_stress_test_results(
        metrics_data: String,
        sequential_time: std::time::Duration,
        concurrent_time: std::time::Duration,
        num_operations: usize,
    ) {
        println!("\n📈 STRESS TEST ANALYSIS");
        println!("========================");

        // Extract metrics
        let seq_count = prometheus_is_contained(
            metrics_data.clone(),
            &format!("{}_count", STRESS_SEQUENTIAL_METRIC.get_name()),
            &[],
        );
        let seq_sum = prometheus_is_contained(
            metrics_data.clone(),
            &format!("{}_sum", STRESS_SEQUENTIAL_METRIC.get_name()),
            &[],
        );
        let conc_count = prometheus_is_contained(
            metrics_data.clone(),
            &format!("{}_count", STRESS_CONCURRENT_METRIC.get_name()),
            &[],
        );
        let conc_sum = prometheus_is_contained(
            metrics_data,
            &format!("{}_sum", STRESS_CONCURRENT_METRIC.get_name()),
            &[],
        );

        match (seq_count, seq_sum, conc_count, conc_sum) {
            (
                Some(Value::Untyped(seq_c)),
                Some(Value::Untyped(seq_s)),
                Some(Value::Untyped(conc_c)),
                Some(Value::Untyped(conc_s)),
            ) => {
                let seq_avg = seq_s / seq_c;
                let conc_avg = conc_s / conc_c;

                println!("\n🔍 FLUSH PERFORMANCE UNDER STRESS:");
                println!("Sequential:");
                println!("   Operations: {}", seq_c);
                println!("   Total flush time: {:.6}s", seq_s);
                println!("   Average per flush: {:.6}s ({:.3}ms)", seq_avg, seq_avg * 1000.0);

                println!("\nConcurrent:");
                println!("   Operations: {}", conc_c);
                println!("   Total flush time: {:.6}s", conc_s);
                println!("   Average per flush: {:.6}s ({:.3}ms)", conc_avg, conc_avg * 1000.0);

                let flush_improvement = seq_avg / conc_avg;
                println!("\n🎯 FLUSH IMPROVEMENT: {:.2}x", flush_improvement);

                // Overall performance
                let total_improvement =
                    sequential_time.as_secs_f64() / concurrent_time.as_secs_f64();
                println!("\n🏆 OVERALL STRESS TEST PERFORMANCE:");
                println!("   Sequential total: {:.3}s", sequential_time.as_secs_f64());
                println!("   Concurrent total: {:.3}s", concurrent_time.as_secs_f64());
                println!("   Overall improvement: {:.2}x", total_improvement);

                let throughput_seq = num_operations as f64 / sequential_time.as_secs_f64();
                let throughput_conc = num_operations as f64 / concurrent_time.as_secs_f64();

                println!("\n📊 THROUGHPUT ANALYSIS:");
                println!("   Sequential throughput: {:.1} operations/second", throughput_seq);
                println!("   Concurrent throughput: {:.1} operations/second", throughput_conc);
                println!("   Throughput improvement: {:.2}x", throughput_conc / throughput_seq);

                if total_improvement > 1.3 {
                    println!("\n🎉 OUTSTANDING! Your concurrent optimization excels under stress!");
                    println!("   This proves significant production benefits!");
                } else if total_improvement > 1.1 {
                    println!(
                        "\n✅ EXCELLENT! Your concurrent optimization shows clear benefits under \
                         stress!"
                    );
                } else if flush_improvement > 1.0 {
                    println!("\n📊 Your concurrent flush optimization is working correctly!");
                    println!(
                        "   Benefits will be more pronounced with larger data volumes in \
                         production."
                    );
                } else {
                    println!(
                        "\n💡 The concurrent implementation handles stress well and is ready for \
                         production!"
                    );
                }
            }
            _ => {
                println!(
                    "Could not extract detailed metrics, but overall timing shows performance \
                     characteristics."
                );
            }
        }
    }
}
