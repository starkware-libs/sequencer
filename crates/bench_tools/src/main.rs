use std::path::PathBuf;

use bench_tools::gcs;
use bench_tools::types::benchmark_config::{
    find_benchmark_by_name,
    find_benchmarks_by_package,
    BENCHMARKS,
};
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
        /// Optional: Local directory containing input files. If not provided, inputs will be
        /// downloaded from GCS for benchmarks that require them.
        #[arg(long)]
        input_dir: Option<String>,
    },
    /// List benchmarks for a package.
    List {
        /// Package name to list benchmarks for. If not provided, lists all benchmarks.
        #[arg(short, long)]
        package: Option<String>,
    },
    /// Upload benchmark input files to GCS.
    UploadInputs {
        /// Benchmark name.
        #[arg(long)]
        benchmark: String,
        /// Local directory containing input files.
        #[arg(long)]
        input_dir: String,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Run { package, out, input_dir } => {
            let benchmarks = find_benchmarks_by_package(&package);

            if benchmarks.is_empty() {
                panic!("No benchmarks found for package: {}", package);
            }

            bench_tools::runner::run_benchmarks(&benchmarks, input_dir.as_deref(), &out);
        }
        Commands::List { package } => match package {
            Some(package_name) => {
                let benchmarks = find_benchmarks_by_package(&package_name);

                if benchmarks.is_empty() {
                    println!("No benchmarks found for package: {}", package_name);
                    return;
                }

                println!("Available benchmarks for package '{}':", package_name);
                for bench in &benchmarks {
                    println!("  - {} (runs: {})", bench.name, bench.cmd_args.join(" "));
                }
            }
            None => {
                println!("All available benchmarks:");
                for bench in BENCHMARKS {
                    println!(
                        "  - {} (package: {}, runs: {})",
                        bench.name,
                        bench.package,
                        bench.cmd_args.join(" ")
                    );
                }
            }
        },
        Commands::UploadInputs { benchmark, input_dir } => {
            // Validate benchmark exists.
            if find_benchmark_by_name(&benchmark).is_none() {
                panic!("Unknown benchmark: {}", benchmark);
            }

            let input_path = PathBuf::from(&input_dir);
            gcs::upload_inputs(&benchmark, &input_path);

            println!("Input files uploaded successfully!");
        }
    }
}
