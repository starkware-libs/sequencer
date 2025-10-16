/// Configuration for a single benchmark.
#[derive(Debug, Clone)]
pub struct BenchmarkConfig {
    pub name: &'static str,
    pub package: &'static str,
    pub cmd_args: &'static [&'static str],
}

impl BenchmarkConfig {
    /// Get the full cargo bench command as owned strings.
    pub fn cmd_args_owned(&self) -> Vec<String> {
        self.cmd_args.iter().map(|s| s.to_string()).collect()
    }
}

/// All available benchmarks defined as a const array.
pub const BENCHMARKS: &[BenchmarkConfig] = &[
    BenchmarkConfig {
        name: "full_committer_flow",
        package: "starknet_committer_and_os_cli",
        cmd_args: &["bench", "-p", "starknet_committer_and_os_cli", "full_committer_flow"],
    },
    BenchmarkConfig {
        name: "single_tree_flow",
        package: "starknet_committer_and_os_cli",
        cmd_args: &["bench", "-p", "starknet_committer_and_os_cli", "single_tree_flow"],
    },
    BenchmarkConfig {
        name: "gateway_apply_block",
        package: "apollo_gateway",
        cmd_args: &["bench", "-p", "apollo_gateway", "apply_block"],
    },
];

/// Helper functions for working with benchmarks.
pub fn find_benchmark_by_name(name: &str) -> Option<&'static BenchmarkConfig> {
    BENCHMARKS.iter().find(|b| b.name == name)
}

pub fn find_benchmarks_by_package(package: &str) -> Vec<&'static BenchmarkConfig> {
    BENCHMARKS.iter().filter(|b| b.package == package).collect()
}
