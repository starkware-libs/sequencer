use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
/// Arguments from the runner, not meant to be set by the user.
pub struct RunnerArgs {
    /// ID for Prometheus logging
    #[arg(short, long, env)]
    pub id: u64,

    /// The port to run the Prometheus metrics server on
    #[arg(long, env)]
    pub metric_port: u16,

    /// The port to run the P2P network on
    #[arg(short, env, long)]
    pub p2p_port: u16,

    /// The addresses of the bootstrap peers (can specify multiple)
    #[arg(long, env, value_delimiter = ',')]
    pub bootstrap: Vec<String>,
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
