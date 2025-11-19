use tracing::Level;

pub const DEFAULT_INTERFERENCE_CONCURRENCY_LIMIT: usize = 20;

pub struct FlavorFields {
    /// Seed for the random number generator.
    pub seed: u64,

    /// Number of iterations to run the benchmark.
    pub n_iterations: usize,

    /// Benchmark flavor determines the size and structure of the generated state diffs.
    pub flavor: BenchmarkFlavor,

    /// Interference flavor determines the type and concurrency of the interference tasks.
    /// Only applicable if the storage supports interference (parallel access).
    pub interference_fields: InterferenceFields,

    /// Number of updates per iteration, where applicable. Different flavors treat this value
    /// differently, see [BenchmarkFlavor] for more details.
    pub n_updates: usize,

    /// Interval at which to save checkpoints.
    pub checkpoint_interval: usize,

    /// Log level.
    pub log_level: Level,
}

/// Specific flavors of workloads to run in the benchmark.
#[derive(Default)]
pub enum BenchmarkFlavor {
    // Constant number of updates per iteration.
    #[default]
    Constant,
    // Periodic peaks of a constant number of updates per peak iteration, with 20% of the number
    // of updates on non-peak iterations. Peaks are 10 iterations every 500 iterations.
    PeriodicPeaks,
    // Constant number of state diffs per iteration, with 20% new leaves per iteration. The other
    // 80% leaf updates are sampled randomly from recent leaf updates.
    // For the first blocks, behaves just like [Self::Constant] ("warmup" phase).
    Overlap,
    // Constant number of updates per iteration, where block N generates updates for leaf keys
    // [N * C, (N + 1) * C).
    Continuous,
}

#[derive(Default, PartialEq)]
pub enum InterferenceFlavor {
    // No interference.
    #[default]
    None,
    // Read 1000 random keys every block.
    Read1KEveryBlock,
}

/// Settings for interference (spawned tasks that run in parallel to the main benchmark).
pub struct InterferenceFields {
    // The type of interference to apply.
    pub interference_type: InterferenceFlavor,

    // The maximum number of interference tasks to run concurrently.
    // Any attempt to spawn a new interference task will log a warning and not spawn the task.
    pub interference_concurrency_limit: usize,
}

impl Default for InterferenceFields {
    fn default() -> Self {
        Self {
            interference_type: InterferenceFlavor::None,
            interference_concurrency_limit: DEFAULT_INTERFERENCE_CONCURRENCY_LIMIT,
        }
    }
}
