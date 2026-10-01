use apollo_integration_tests::flows;
use clap::{Parser, ValueEnum};

#[derive(Parser)]
#[command(name = "integration_test_runner", about = "Run one sequencer integration-test flow.")]
struct Args {
    /// The integration-test flow to run.
    flow: Flow,
}

#[derive(Clone, ValueEnum)]
#[value(rename_all = "snake_case")]
enum Flow {
    Positive,
    Proof,
    Restart,
    RestartMultipleNodes,
    RestartSingleNode,
    Revert,
    Sync,
}

#[tokio::main]
async fn main() {
    // The match is exhaustive by design: adding a `Flow` variant without wiring its module is a
    // compile error, so the enum is the single source of truth for the set of flows. clap parses
    // the flow name, rejects unknown names with the valid list, and exits non-zero on bad usage
    // (distinct from a flow failing, which panics).
    match Args::parse().flow {
        Flow::Positive => flows::positive::run().await,
        Flow::Proof => flows::proof::run().await,
        Flow::Restart => flows::restart::run().await,
        Flow::RestartMultipleNodes => flows::restart_multiple_nodes::run().await,
        Flow::RestartSingleNode => flows::restart_single_node::run().await,
        Flow::Revert => flows::revert::run().await,
        Flow::Sync => flows::sync::run().await,
    }
}
