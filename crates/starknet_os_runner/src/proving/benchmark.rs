//! Benchmark harness for comparing proving methods.
//!
//! Provides a hyperfine-like framework for benchmarking and comparing proving methods:
//! 1. Direct: Uses `stwo_direct::run_and_prove_virtual_os` which runs and proves in one step.
//! 2. Via PIE: Uses `starknet_os::runner::run_virtual_os` then `prover::prove` in two steps.
//! 3. In-Memory: Uses `run_virtual_os` then `prover::prove_in_memory` (avoids writing PIE to disk).
//!
//! # Running
//!
//! ```bash
//! SEPOLIA_NODE_URL=https://your-rpc-node \
//! rustup run nightly-2025-07-14 cargo test -p starknet_os_runner \
//!   --features stwo_native,in_memory_proving \
//!   test_benchmark_proving_methods \
//!   -- --ignored --nocapture
//! ```

use std::fmt;
use std::time::{Duration, Instant};

/// Timing results for a single step of the proving process.
#[derive(Debug, Clone)]
pub struct StepTiming {
    pub name: String,
    pub duration: Duration,
}

/// Aggregate timing results for a proving method run.
#[derive(Debug, Clone)]
pub struct MethodTiming {
    pub method_name: String,
    pub steps: Vec<StepTiming>,
    pub total_duration: Duration,
}

impl MethodTiming {
    /// Creates a new timing result with empty steps.
    pub fn new(method_name: &str) -> Self {
        Self { method_name: method_name.to_string(), steps: Vec::new(), total_duration: Duration::ZERO }
    }

    /// Adds a step timing.
    pub fn add_step(&mut self, name: &str, duration: Duration) {
        self.steps.push(StepTiming { name: name.to_string(), duration });
    }

    /// Computes the total duration from steps.
    pub fn finalize(&mut self) {
        self.total_duration = self.steps.iter().map(|s| s.duration).sum();
    }
}

/// Statistics for multiple runs of a benchmark.
#[derive(Debug, Clone)]
pub struct BenchmarkStats {
    pub method_name: String,
    pub runs: Vec<MethodTiming>,
    pub mean: Duration,
    pub min: Duration,
    pub max: Duration,
    pub stddev: Duration,
}

impl BenchmarkStats {
    /// Computes statistics from a list of timings.
    #[allow(clippy::as_conversions)] // Casting to f64 for statistics is expected.
    pub fn from_runs(method_name: &str, runs: Vec<MethodTiming>) -> Self {
        let durations: Vec<Duration> = runs.iter().map(|r| r.total_duration).collect();
        let n = durations.len() as u64;

        let sum: Duration = durations.iter().sum();
        let mean = sum / n as u32;

        let min = *durations.iter().min().unwrap_or(&Duration::ZERO);
        let max = *durations.iter().max().unwrap_or(&Duration::ZERO);

        // Compute standard deviation.
        let variance_nanos: f64 = if n > 1 {
            let mean_nanos = mean.as_nanos() as f64;
            let sum_sq: f64 =
                durations.iter().map(|d| (d.as_nanos() as f64 - mean_nanos).powi(2)).sum();
            sum_sq / (n - 1) as f64
        } else {
            0.0
        };
        let stddev = Duration::from_nanos(variance_nanos.sqrt() as u64);

        Self { method_name: method_name.to_string(), runs, mean, min, max, stddev }
    }
}

/// Comparison result between two benchmark runs.
#[derive(Debug)]
pub struct BenchmarkComparison {
    pub baseline: BenchmarkStats,
    pub contender: BenchmarkStats,
    /// Speedup ratio (baseline mean / contender mean). > 1 means contender is faster.
    pub speedup: f64,
    /// Percentage difference ((baseline - contender) / baseline * 100).
    pub percentage_diff: f64,
}

impl BenchmarkComparison {
    /// Creates a comparison between baseline and contender.
    #[allow(clippy::as_conversions)] // Casting to f64 for statistics is expected.
    pub fn compare(baseline: BenchmarkStats, contender: BenchmarkStats) -> Self {
        let baseline_nanos = baseline.mean.as_nanos() as f64;
        let contender_nanos = contender.mean.as_nanos() as f64;

        let speedup = if contender_nanos > 0.0 { baseline_nanos / contender_nanos } else { 0.0 };

        let percentage_diff =
            if baseline_nanos > 0.0 { (baseline_nanos - contender_nanos) / baseline_nanos * 100.0 } else { 0.0 };

        Self { baseline, contender, speedup, percentage_diff }
    }
}

impl fmt::Display for BenchmarkComparison {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "\n{}", "=".repeat(80))?;
        writeln!(f, "BENCHMARK COMPARISON: {} vs {}", self.baseline.method_name, self.contender.method_name)?;
        writeln!(f, "{}", "=".repeat(80))?;

        // Display baseline stats.
        writeln!(f, "\n{} (baseline):", self.baseline.method_name)?;
        writeln!(f, "  Mean:   {:>12.3?}", self.baseline.mean)?;
        writeln!(f, "  Min:    {:>12.3?}", self.baseline.min)?;
        writeln!(f, "  Max:    {:>12.3?}", self.baseline.max)?;
        writeln!(f, "  StdDev: {:>12.3?}", self.baseline.stddev)?;

        // Display contender stats.
        writeln!(f, "\n{} (contender):", self.contender.method_name)?;
        writeln!(f, "  Mean:   {:>12.3?}", self.contender.mean)?;
        writeln!(f, "  Min:    {:>12.3?}", self.contender.min)?;
        writeln!(f, "  Max:    {:>12.3?}", self.contender.max)?;
        writeln!(f, "  StdDev: {:>12.3?}", self.contender.stddev)?;

        // Display comparison.
        writeln!(f, "\nComparison:")?;
        writeln!(f, "  Speedup: {:.3}x", self.speedup)?;
        if self.percentage_diff > 0.0 {
            writeln!(f, "  {} is {:.1}% faster than {}", self.contender.method_name, self.percentage_diff, self.baseline.method_name)?;
        } else if self.percentage_diff < 0.0 {
            writeln!(
                f,
                "  {} is {:.1}% slower than {}",
                self.contender.method_name,
                -self.percentage_diff,
                self.baseline.method_name
            )?;
        } else {
            writeln!(f, "  Both methods have the same performance")?;
        }

        // Step-by-step breakdown if available.
        if !self.baseline.runs.is_empty() && !self.contender.runs.is_empty() {
            writeln!(f, "\nStep-by-step breakdown (first run):")?;
            writeln!(f, "{}", "-".repeat(80))?;

            let baseline_run = &self.baseline.runs[0];
            let contender_run = &self.contender.runs[0];

            writeln!(f, "\n{}:", baseline_run.method_name)?;
            for step in &baseline_run.steps {
                writeln!(f, "  {:<40} {:>12.3?}", step.name, step.duration)?;
            }

            writeln!(f, "\n{}:", contender_run.method_name)?;
            for step in &contender_run.steps {
                writeln!(f, "  {:<40} {:>12.3?}", step.name, step.duration)?;
            }
        }

        writeln!(f, "\n{}", "=".repeat(80))?;
        Ok(())
    }
}

/// Timer helper for measuring step durations.
pub struct StepTimer {
    start: Instant,
}

impl StepTimer {
    /// Starts a new timer.
    pub fn start() -> Self {
        Self { start: Instant::now() }
    }

    /// Stops the timer and returns the elapsed duration.
    pub fn stop(&self) -> Duration {
        self.start.elapsed()
    }
}

#[cfg(test)]
mod tests {
    use blockifier_reexecution::state_reader::rpc_objects::BlockId;
    use rstest::rstest;
    use starknet_api::abi::abi_utils::selector_from_name;
    use starknet_api::block::GasPrice;
    use starknet_api::core::{ChainId, ContractAddress};
    use starknet_api::execution_resources::GasAmount;
    use starknet_api::test_utils::invoke::{invoke_tx};
    use starknet_api::transaction::fields::{AllResourceBounds, ResourceBounds, ValidResourceBounds};
    use starknet_api::transaction::{InvokeTransaction, Transaction, TransactionHash};
    use starknet_api::{calldata, felt, invoke_tx_args};
    use starknet_os::runner::run_virtual_os;
    use tempfile::NamedTempFile;

    use super::*;
    use crate::proving::prover::prove;
    #[cfg(feature = "in_memory_proving")]
    use crate::proving::prover::prove_in_memory;
    use crate::proving::stwo_direct::{run_and_prove_virtual_os, StwoDirectProvingConfig};
    use crate::runner::RpcRunnerFactory;
    use crate::test_utils::{
        fetch_sepolia_block_number,
        sepolia_runner_factory,
        DUMMY_ACCOUNT_ADDRESS,
        STRK_TOKEN_ADDRESS_SEPOLIA,
    };

    /// Creates an invoke transaction that calls `balanceOf` on the STRK token.
    fn strk_balance_of_invoke() -> (InvokeTransaction, TransactionHash) {
        let strk_token = ContractAddress::try_from(STRK_TOKEN_ADDRESS_SEPOLIA).unwrap();
        let account = ContractAddress::try_from(DUMMY_ACCOUNT_ADDRESS).unwrap();

        let calldata = calldata![
            *strk_token.0.key(),
            selector_from_name("balanceOf").0,
            felt!("1"),
            *account.0.key()
        ];

        let resource_bounds = ValidResourceBounds::AllResources(AllResourceBounds {
            l1_gas: ResourceBounds { max_amount: GasAmount(0), max_price_per_unit: GasPrice(0) },
            l2_gas: ResourceBounds {
                max_amount: GasAmount(10_000_000),
                max_price_per_unit: GasPrice(0),
            },
            l1_data_gas: ResourceBounds {
                max_amount: GasAmount(0),
                max_price_per_unit: GasPrice(0),
            },
        });

        let invoke = invoke_tx(invoke_tx_args! {
            sender_address: account,
            calldata,
            resource_bounds,
        });

        let tx_hash = Transaction::Invoke(invoke.clone())
            .calculate_transaction_hash(&ChainId::Sepolia)
            .unwrap();

        (invoke, tx_hash)
    }

    /// Runs the direct proving method and returns timing information.
    async fn run_direct_method(
        runner_factory: &RpcRunnerFactory,
        block_number: starknet_api::block::BlockNumber,
    ) -> MethodTiming {
        let mut timing = MethodTiming::new("Direct (stwo_direct)");

        // Create transaction.
        let timer = StepTimer::start();
        let (tx, tx_hash) = strk_balance_of_invoke();
        timing.add_step("Create transaction", timer.stop());

        // Create OS hints.
        let timer = StepTimer::start();
        let runner = runner_factory.create_runner(BlockId::Number(block_number));
        let os_hints = runner
            .create_virtual_os_hints(vec![(tx, tx_hash)])
            .await
            .expect("create_virtual_os_hints should succeed");
        timing.add_step("Create OS hints", timer.stop());

        // Create temp file for proof output.
        let proof_file = NamedTempFile::new().expect("Failed to create temp file");
        let proof_path = proof_file.path().to_path_buf();

        // Configure proving.
        let proving_config = StwoDirectProvingConfig {
            bootloader_program_path: None,
            proof_output_path: proof_path.clone(),
            verify: false,
            prover_params_path: None,
            debug_data_dir: None,
            save_debug_data: false,
        };

        // Run and prove.
        let timer = StepTimer::start();
        run_and_prove_virtual_os(os_hints, proving_config)
            .expect("run_and_prove_virtual_os should succeed");
        timing.add_step("Run and prove (combined)", timer.stop());

        // Verify proof file was created.
        let metadata = std::fs::metadata(&proof_path).expect("Proof file should exist");
        assert!(metadata.len() > 0, "Proof file should not be empty");

        timing.finalize();
        timing
    }

    /// Runs the via-PIE proving method and returns timing information.
    async fn run_via_pie_method(
        runner_factory: &RpcRunnerFactory,
        block_number: starknet_api::block::BlockNumber,
    ) -> MethodTiming {
        let mut timing = MethodTiming::new("Via PIE (run_virtual_os + prove)");

        // Create transaction.
        let timer = StepTimer::start();
        let (tx, tx_hash) = strk_balance_of_invoke();
        timing.add_step("Create transaction", timer.stop());

        // Create OS hints.
        let timer = StepTimer::start();
        let runner = runner_factory.create_runner(BlockId::Number(block_number));
        let os_hints = runner
            .create_virtual_os_hints(vec![(tx, tx_hash)])
            .await
            .expect("create_virtual_os_hints should succeed");
        timing.add_step("Create OS hints", timer.stop());

        // Run virtual OS.
        let timer = StepTimer::start();
        let runner_output = run_virtual_os(os_hints).expect("run_virtual_os should succeed");
        timing.add_step("Run virtual OS", timer.stop());

        // Prove CairoPie.
        let timer = StepTimer::start();
        let _prover_output = prove(runner_output.cairo_pie).await.expect("prove should succeed");
        timing.add_step("Prove CairoPie", timer.stop());

        timing.finalize();
        timing
    }

    /// Runs the in-memory proving method and returns timing information.
    #[cfg(feature = "in_memory_proving")]
    async fn run_in_memory_method(
        runner_factory: &RpcRunnerFactory,
        block_number: starknet_api::block::BlockNumber,
    ) -> MethodTiming {
        let mut timing = MethodTiming::new("In-Memory (run_virtual_os + prove_in_memory)");

        // Create transaction.
        let timer = StepTimer::start();
        let (tx, tx_hash) = strk_balance_of_invoke();
        timing.add_step("Create transaction", timer.stop());

        // Create OS hints.
        let timer = StepTimer::start();
        let runner = runner_factory.create_runner(BlockId::Number(block_number));
        let os_hints = runner
            .create_virtual_os_hints(vec![(tx, tx_hash)])
            .await
            .expect("create_virtual_os_hints should succeed");
        timing.add_step("Create OS hints", timer.stop());

        // Run virtual OS.
        let timer = StepTimer::start();
        let runner_output = run_virtual_os(os_hints).expect("run_virtual_os should succeed");
        timing.add_step("Run virtual OS", timer.stop());

        // Prove CairoPie in-memory (no disk write for PIE).
        let timer = StepTimer::start();
        let _prover_output =
            prove_in_memory(runner_output.cairo_pie).expect("prove_in_memory should succeed");
        timing.add_step("Prove CairoPie (in-memory)", timer.stop());

        timing.finalize();
        timing
    }

    /// Benchmark test comparing all proving methods.
    ///
    /// # Running
    ///
    /// ```bash
    /// SEPOLIA_NODE_URL=https://your-rpc-node \
    /// rustup run nightly-2025-07-14 cargo test -p starknet_os_runner \
    ///   --features stwo_native,in_memory_proving \
    ///   test_benchmark_proving_methods \
    ///   -- --ignored --nocapture
    /// ```
    #[rstest]
    #[tokio::test(flavor = "multi_thread")]
    #[ignore] // Requires RPC access and nightly Rust.
    async fn test_benchmark_proving_methods(sepolia_runner_factory: RpcRunnerFactory) {
        const NUM_RUNS: usize = 10; // Configurable number of runs.

        println!("\n{}", "=".repeat(80));
        println!("PROVING METHODS BENCHMARK");
        println!("{}", "=".repeat(80));
        println!("Number of runs per method: {NUM_RUNS}");

        // Fetch block number once for all runs.
        let timer = StepTimer::start();
        let block_number = fetch_sepolia_block_number().await;
        println!("\nFetched block number {} in {:?}", block_number, timer.stop());

        // Run direct method benchmarks.
        println!("\n--- Running Direct method benchmarks ---");
        let mut direct_runs = Vec::new();
        for i in 0..NUM_RUNS {
            println!("  Run {}/{}...", i + 1, NUM_RUNS);
            let timing = run_direct_method(&sepolia_runner_factory, block_number).await;
            println!("  Completed in {:?}", timing.total_duration);
            direct_runs.push(timing);
        }

        // Run via-PIE method benchmarks.
        println!("\n--- Running Via-PIE method benchmarks ---");
        let mut via_pie_runs = Vec::new();
        for i in 0..NUM_RUNS {
            println!("  Run {}/{}...", i + 1, NUM_RUNS);
            let timing = run_via_pie_method(&sepolia_runner_factory, block_number).await;
            println!("  Completed in {:?}", timing.total_duration);
            via_pie_runs.push(timing);
        }

        // Run in-memory method benchmarks (if feature enabled).
        #[cfg(feature = "in_memory_proving")]
        let in_memory_runs = {
            println!("\n--- Running In-Memory method benchmarks ---");
            let mut runs = Vec::new();
            for i in 0..NUM_RUNS {
                println!("  Run {}/{}...", i + 1, NUM_RUNS);
                let timing = run_in_memory_method(&sepolia_runner_factory, block_number).await;
                println!("  Completed in {:?}", timing.total_duration);
                runs.push(timing);
            }
            runs
        };

        // Compute statistics.
        let direct_stats = BenchmarkStats::from_runs("Direct (stwo_direct)", direct_runs);
        let via_pie_stats = BenchmarkStats::from_runs("Via PIE", via_pie_runs);
        #[cfg(feature = "in_memory_proving")]
        let in_memory_stats = BenchmarkStats::from_runs("In-Memory", in_memory_runs);

        // Display results.
        println!("\n{}", "=".repeat(80));
        println!("BENCHMARK RESULTS SUMMARY");
        println!("{}", "=".repeat(80));

        // Display all stats.
        for stats in [&direct_stats, &via_pie_stats] {
            println!("\n{}:", stats.method_name);
            println!("  Mean:   {:>12.3?}", stats.mean);
            println!("  Min:    {:>12.3?}", stats.min);
            println!("  Max:    {:>12.3?}", stats.max);
            println!("  StdDev: {:>12.3?}", stats.stddev);
        }
        #[cfg(feature = "in_memory_proving")]
        {
            println!("\n{}:", in_memory_stats.method_name);
            println!("  Mean:   {:>12.3?}", in_memory_stats.mean);
            println!("  Min:    {:>12.3?}", in_memory_stats.min);
            println!("  Max:    {:>12.3?}", in_memory_stats.max);
            println!("  StdDev: {:>12.3?}", in_memory_stats.stddev);
        }

        // Compare Via-PIE vs Direct.
        let comparison = BenchmarkComparison::compare(via_pie_stats.clone(), direct_stats.clone());
        println!("{comparison}");

        // Compare In-Memory vs Via-PIE (if feature enabled).
        #[cfg(feature = "in_memory_proving")]
        {
            let comparison = BenchmarkComparison::compare(via_pie_stats, in_memory_stats);
            println!("{comparison}");
        }
    }

    /// Single-run comparison test for quick testing.
    ///
    /// # Running
    ///
    /// ```bash
    /// SEPOLIA_NODE_URL=https://your-rpc-node \
    /// rustup run nightly-2025-07-14 cargo test -p starknet_os_runner \
    ///   --features stwo_native,in_memory_proving \
    ///   test_quick_comparison \
    ///   -- --ignored --nocapture
    /// ```
    #[rstest]
    #[tokio::test(flavor = "multi_thread")]
    #[ignore] // Requires RPC access and nightly Rust.
    #[allow(clippy::as_conversions)] // Casting to f64 for statistics is expected.
    async fn test_quick_comparison(sepolia_runner_factory: RpcRunnerFactory) {
        println!("\n{}", "=".repeat(80));
        println!("QUICK PROVING METHODS COMPARISON (Single Run)");
        println!("{}", "=".repeat(80));

        // Fetch block number.
        let block_number = fetch_sepolia_block_number().await;
        println!("Using block number: {block_number}");

        // Run all methods.
        println!("\n--- Direct method ---");
        let direct_timing = run_direct_method(&sepolia_runner_factory, block_number).await;
        for step in &direct_timing.steps {
            println!("  {:<40} {:>12.3?}", step.name, step.duration);
        }
        println!("  {:<40} {:>12.3?}", "TOTAL", direct_timing.total_duration);

        println!("\n--- Via-PIE method ---");
        let via_pie_timing = run_via_pie_method(&sepolia_runner_factory, block_number).await;
        for step in &via_pie_timing.steps {
            println!("  {:<40} {:>12.3?}", step.name, step.duration);
        }
        println!("  {:<40} {:>12.3?}", "TOTAL", via_pie_timing.total_duration);

        #[cfg(feature = "in_memory_proving")]
        let in_memory_timing = {
            println!("\n--- In-Memory method ---");
            let timing = run_in_memory_method(&sepolia_runner_factory, block_number).await;
            for step in &timing.steps {
                println!("  {:<40} {:>12.3?}", step.name, step.duration);
            }
            println!("  {:<40} {:>12.3?}", "TOTAL", timing.total_duration);
            timing
        };

        // Simple comparison.
        let direct_nanos = direct_timing.total_duration.as_nanos() as f64;
        let via_pie_nanos = via_pie_timing.total_duration.as_nanos() as f64;

        println!("\n{}", "-".repeat(80));
        println!("COMPARISON (Direct vs Via-PIE):");
        let speedup = via_pie_nanos / direct_nanos;
        if speedup > 1.0 {
            println!(
                "  Direct is {:.2}x faster ({:.1}% improvement)",
                speedup,
                (1.0 - direct_nanos / via_pie_nanos) * 100.0
            );
        } else if speedup < 1.0 {
            println!(
                "  Via-PIE is {:.2}x faster ({:.1}% improvement)",
                1.0 / speedup,
                (1.0 - via_pie_nanos / direct_nanos) * 100.0
            );
        } else {
            println!("  Both methods have equivalent performance");
        }

        #[cfg(feature = "in_memory_proving")]
        {
            let in_memory_nanos = in_memory_timing.total_duration.as_nanos() as f64;
            println!("\nCOMPARISON (In-Memory vs Via-PIE):");
            let speedup = via_pie_nanos / in_memory_nanos;
            if speedup > 1.0 {
                println!(
                    "  In-Memory is {:.2}x faster ({:.1}% improvement)",
                    speedup,
                    (1.0 - in_memory_nanos / via_pie_nanos) * 100.0
                );
            } else if speedup < 1.0 {
                println!(
                    "  Via-PIE is {:.2}x faster ({:.1}% improvement)",
                    1.0 / speedup,
                    (1.0 - via_pie_nanos / in_memory_nanos) * 100.0
                );
            } else {
                println!("  Both methods have equivalent performance");
            }
        }

        println!("{}", "=".repeat(80));
    }
}
