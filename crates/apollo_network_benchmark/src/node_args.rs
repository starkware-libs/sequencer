use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
/// Arguments from the runner, not meant to be set by the user.
pub struct RunnerArgs {
    /// The port to run the Prometheus metrics server on
    #[arg(long, env)]
    pub metric_port: u16,
}

#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
/// Arguments from the user.
pub struct UserArgs {
    /// Set the verbosity level of the logger, the higher the more verbose
    #[arg(short, long, env, default_value = "2")]
    pub verbosity: u8,

    /// The timeout in seconds for the node.
    /// When the node runs for longer than this, it will be killed.
    #[arg(long, env, default_value = "4000")]
    pub timeout: u64,
}

#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
pub struct NodeArgs {
    #[command(flatten)]
    pub runner: RunnerArgs,
    #[command(flatten)]
    pub user: UserArgs,
}
