// Utility modules
mod args;
mod grafana_config;
mod mod_utils;
mod yaml_maker;

// Command modules
mod cluster_logs;
mod cluster_port_forward;
mod cluster_start;
mod cluster_stop;
mod local_start;
mod local_stop;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "run")]
#[command(about = "Broadcast Network Stress Test - Run local or cluster deployments")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Local deployment commands
    #[command(subcommand)]
    Local(LocalCommands),

    /// Cluster management commands
    #[command(subcommand)]
    Cluster(ClusterCommands),
}

#[derive(Subcommand)]
enum LocalCommands {
    /// Start a new local deployment
    Start(local_start::LocalStartArgs),

    /// Stop and cleanup the local deployment
    Stop,
}

#[derive(Subcommand)]
enum ClusterCommands {
    /// Start a new cluster deployment
    Start(cluster_start::ClusterStartArgs),

    /// Stop and cleanup the cluster deployment
    Stop,

    /// Fetch logs from cluster pods
    Logs,

    /// Port forward Grafana and Prometheus services
    PortForward,
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Local(local_cmd) => match local_cmd {
            LocalCommands::Start(args) => local_start::run(args),
            LocalCommands::Stop => local_stop::run(),
        },
        Commands::Cluster(cluster_cmd) => match cluster_cmd {
            ClusterCommands::Start(args) => cluster_start::run(args),
            ClusterCommands::Stop => cluster_stop::run(),
            ClusterCommands::Logs => cluster_logs::run(),
            ClusterCommands::PortForward => cluster_port_forward::run(),
        },
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
