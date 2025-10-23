/// Configuration for a single benchmark.
#[derive(Debug, Clone)]
pub struct BenchmarkConfig {
    pub name: &'static str,
    pub package: &'static str,
    pub cmd_args: &'static [&'static str],
    /// Optional input directory path relative to workspace root. If set, inputs will be
    /// downloaded from GCS before running the benchmark.
    pub input_dir: Option<&'static str>,
}

impl BenchmarkConfig {
    /// Get the full cargo bench command as owned strings.
    pub fn cmd_args_owned(&self) -> Vec<String> {
        self.cmd_args.iter().map(|s| s.to_string()).collect()
    }

    /// Check if this benchmark requires input files.
    pub fn needs_inputs(&self) -> bool {
        self.input_dir.is_some()
    }
}

/// All available benchmarks defined as a const array.
pub const BENCHMARKS: &[BenchmarkConfig] = &[
    BenchmarkConfig {
        name: "full_committer_flow",
        package: "starknet_committer_and_os_cli",
        cmd_args: &["bench", "-p", "starknet_committer_and_os_cli", "full_committer_flow"],
        input_dir: Some("crates/starknet_committer_and_os_cli/test_inputs"),
    },
    BenchmarkConfig {
        name: "single_tree_flow",
        package: "starknet_committer_and_os_cli",
        cmd_args: &["bench", "-p", "starknet_committer_and_os_cli", "single_tree_flow"],
        input_dir: Some("crates/starknet_committer_and_os_cli/test_inputs"),
    },
    BenchmarkConfig {
        name: "gateway_apply_block",
        package: "apollo_gateway",
        cmd_args: &["bench", "-p", "apollo_gateway", "apply_block"],
        input_dir: None,
    },
    BenchmarkConfig {
        name: "dummy_benchmark",
        package: "bench_tools",
        cmd_args: &["bench", "-p", "bench_tools", "--bench", "dummy_bench"],
        input_dir: Some("crates/bench_tools/data/dummy_bench_input"),
    },
];

/// Helper functions for working with benchmarks.
pub fn find_benchmark_by_name(name: &str) -> Option<&'static BenchmarkConfig> {
    BENCHMARKS.iter().find(|b| b.name == name)
}

pub fn find_benchmarks_by_package(package: &str) -> Vec<&'static BenchmarkConfig> {
    BENCHMARKS.iter().filter(|b| b.package == package).collect()
}
