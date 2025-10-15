use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(about = "Benchmark runner and comparison tool for CI.")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run benchmarks and output results.
    Run {
        /// Package name to run benchmarks for.
        #[arg(short, long)]
        package: String,
        /// Output directory for results.
        #[arg(short, long)]
        out: String,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run { package: _, out: _ } => {
            unimplemented!()
        }
    }
}
