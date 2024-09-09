use std::collections::HashSet;
use std::fs::{self, File};
use std::net::TcpListener;
use std::process::Command;
use std::str::FromStr;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use clap::Parser;
use fs2::FileExt;
use lazy_static::lazy_static;
use tokio::process::Command as TokioCommand;

lazy_static! {
    static ref BOOTNODE_TCP_PORT: u16 = find_free_port();
}
// The SECRET_KEY is used for building the BOOT_NODE_PEER_ID, so they are coupled and must be used
// together.
const SECRET_KEY: &str = "0xabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcd";
const BOOT_NODE_PEER_ID: &str = "12D3KooWDFYi71juk6dYWo3UDvqs5gAzGDc124LSvcR5d187Tdvi";

const MONITORING_PERIOD_SECONDS: u64 = 10;

struct Node {
    validator_id: usize,
    monitoring_gateway_server_port: u16,
    cmd: String,
    process: Option<tokio::process::Child>,
    // Nodes current height and the timestamp it was updated.
    height_and_timestamp: (Option<u64>, Option<Instant>), //(height, timestamp)
    // Number of times the nodes height was updated due to sync, instead of reaching a decision.
    sync_count: Option<u64>,
}

impl Node {
    fn new(validator_id: usize, monitoring_gateway_server_port: u16, cmd: String) -> Self {
        Node {
            validator_id,
            monitoring_gateway_server_port,
            cmd,
            process: None,
            height_and_timestamp: (None, None),
            sync_count: None,
        }
    }

    fn start(&mut self) {
        self.process = Some(
            TokioCommand::new("sh")
                .arg("-c")
                .arg(&self.cmd)
                .spawn()
                .expect("Failed to start process"),
        );
    }

    async fn stop(&mut self) {
        if let Some(process) = self.process.as_mut() {
            process.kill().await.expect("Failed to kill process");
        }
    }

    async fn get_metric(&self, metric: &str) -> Option<u64> {
        let command = format!(
            "curl -s -X GET http://localhost:{}/monitoring/metrics | grep -oP '{} \\K\\d+'",
            self.monitoring_gateway_server_port, metric
        );

        let output =
            Command::new("sh").arg("-c").arg(command).output().expect("Failed to execute command");

        if !output.stdout.is_empty() {
            let metric_value = String::from_utf8_lossy(&output.stdout);
            metric_value.trim().parse().ok()
        } else {
            None
        }
    }

    // Check the node's metrics and return the height and timestamp.
    async fn check_node(&mut self) -> (Option<u64>, Option<Instant>) {
        self.sync_count = self.get_metric("papyrus_consensus_sync_count").await;
        let height = self.get_metric("papyrus_consensus_height").await;

        if self.height_and_timestamp.0 == height {
            return self.height_and_timestamp;
        }
        if let (Some(old_height), Some(new_height)) = (self.height_and_timestamp.0, height) {
            assert!(new_height > old_height, "Height should be increasing.");
        }
        self.height_and_timestamp = (height, Some(Instant::now()));

        self.height_and_timestamp
    }
}

#[derive(Parser)]
#[command(name = "Papyrus CLI")]
struct Cli {
    #[command(flatten)]
    papyrus_args: PapyrusArgs,
    #[command(flatten)]
    run_consensus_args: RunConsensusArgs,
}

#[derive(Parser)]
// Args passed to the test script that are forwarded to the node.
struct PapyrusArgs {
    #[arg(long = "base_layer_node_url")]
    base_layer_node_url: String,
    #[arg(long = "num_validators")]
    num_validators: usize,
    #[arg(long = "db_dir", help = "Directory with existing DBs that this simulation can reuse.")]
    db_dir: Option<String>,
    #[arg(long = "proposal_timeout", help = "The timeout (seconds) for a proposal.")]
    proposal_timeout: Option<f64>,
    #[arg(long = "prevote_timeout", help = "The timeout (seconds) for a prevote.")]
    prevote_timeout: Option<f64>,
    #[arg(long = "precommit_timeout", help = "The timeout (seconds) for a precommit.")]
    precommit_timeout: Option<f64>,
    #[arg(long = "cache_size", help = "Cache size for the test simulation.")]
    cache_size: Option<usize>,
    #[arg(long = "random_seed", help = "Random seed for test simulation.")]
    random_seed: Option<u64>,
    #[arg(
        long = "drop_probability",
        help = "Probability of dropping a message for test simulation."
    )]
    drop_probability: Option<f64>,
    #[arg(
        long = "invalid_probability",
        help = "Probability of sending an invalid message for test simulation."
    )]
    invalid_probability: Option<f64>,
}

#[derive(Parser)]
// Args passed to the script that are not forwarded to the node.
struct RunConsensusArgs {
    #[arg(
        long = "stagnation_threshold",
        help = "Time in seconds to check for height stagnation.",
        default_value = "60", value_parser = parse_duration
    )]
    stagnation_threshold: Duration,
    #[arg(long = "duration", help = "Maximum test duration in seconds.", 
    default_value = "123456789123456789", 
    value_parser = parse_duration)]
    max_test_duration: Duration,
}

struct LockDir {
    file: File,
}

impl LockDir {
    pub fn new(db_dir: &str) -> std::io::Result<Self> {
        let lockfile_path = format!("{}/lockfile", db_dir);
        let file = File::create(&lockfile_path)?;

        match file.try_lock_exclusive() {
            Ok(_) => Ok(LockDir { file }),
            Err(e) => Err(e),
        }
    }
}

impl Drop for LockDir {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

fn parse_duration(s: &str) -> Result<Duration, std::num::ParseIntError> {
    let secs = u64::from_str(s)?;
    Ok(Duration::from_secs(secs))
}

fn find_free_port() -> u16 {
    // The socket is automatically closed when the function exits.
    // The port may still be available when accessed, but this is not guaranteed.
    // TODO(Asmaa): find a reliable way to ensure the port stays free.
    let listener = TcpListener::bind("0.0.0.0:0").expect("Failed to bind");
    listener.local_addr().expect("Failed to get local address").port()
}

// Returns if the simulation should exit.
async fn monitor_simulation(
    nodes: &mut Vec<Node>,
    start_time: Instant,
    max_test_duration: Duration,
    stagnation_timeout: Duration,
) -> bool {
    if start_time.elapsed() > max_test_duration {
        return true;
    }

    let mut stagnated_nodes = Vec::new();
    for node in nodes {
        let (height, last_update) = node.check_node().await;
        println!(
            "Node: {}, height: {:?}, sync_count: {:?}",
            node.validator_id, height, node.sync_count
        );
        // height is None when consensus has not been started yet.
        let elapsed = match height {
            Some(_) => last_update.expect("Must be set if height is set").elapsed(),
            None => start_time.elapsed(),
        };
        if elapsed > stagnation_timeout {
            stagnated_nodes.push(node.validator_id);
        }
    }

    if !stagnated_nodes.is_empty() {
        println!("Nodes {stagnated_nodes:?} have stagnated. Exiting simulation.");
        return true;
    }
    false
}

async fn run_simulation(
    mut nodes: Vec<Node>,
    max_test_duration: Duration,
    stagnation_timeout: Duration,
) {
    for node in nodes.iter_mut() {
        node.start();
    }

    let start_time = Instant::now();

    loop {
        tokio::select! {
            should_break = async {
                tokio::time::sleep(Duration::from_secs(MONITORING_PERIOD_SECONDS)).await;
                let elapsed = start_time.elapsed().as_secs();
                println!("\nTime elapsed: {}s", elapsed);

                monitor_simulation(&mut nodes, start_time, max_test_duration, stagnation_timeout).await
            } => {
                if should_break {
                    break;
                }
            }
            _ = tokio::signal::ctrl_c() => {
                println!("\nTerminating subprocesses...");
                break;
            }
        }
    }

    for mut node in nodes {
        node.stop().await;
    }
}

async fn build_node(data_dir: &str, logs_dir: &str, i: usize, papyrus_args: &PapyrusArgs) -> Node {
    let is_bootstrap = i == 1;
    let tcp_port = if is_bootstrap { *BOOTNODE_TCP_PORT } else { find_free_port() };
    let monitoring_gateway_server_port = find_free_port();
    let data_dir = format!("{}/data{}", data_dir, i);

    let mut cmd = format!(
        "RUST_LOG=papyrus_consensus=debug,papyrus=info target/release/papyrus_node \
         --network.#is_none false --base_layer.node_url {} --storage.db_config.path_prefix {} \
         --consensus.#is_none false --consensus.validator_id 0x{} --consensus.num_validators {} \
         --network.tcp_port {} --rpc.server_address 127.0.0.1:{} \
         --monitoring_gateway.server_address 127.0.0.1:{} --consensus.test.#is_none false \
         --collect_metrics true ",
        papyrus_args.base_layer_node_url,
        data_dir,
        i,
        papyrus_args.num_validators,
        tcp_port,
        find_free_port(),
        monitoring_gateway_server_port
    );

    let conditional_params = [
        ("timeouts.proposal_timeout", papyrus_args.proposal_timeout),
        ("timeouts.prevote_timeout", papyrus_args.prevote_timeout),
        ("timeouts.precommit_timeout", papyrus_args.precommit_timeout),
        ("test.drop_probability", papyrus_args.drop_probability),
        ("test.invalid_probability", papyrus_args.invalid_probability),
        // Convert optional parameters to f64 for consistency in the vector,
        // types were validated during parsing.
        ("test.cache_size", papyrus_args.cache_size.map(|v| v as f64)),
        ("test.random_seed", papyrus_args.random_seed.map(|v| v as f64)),
    ];
    for (key, value) in conditional_params.iter() {
        if let Some(v) = value {
            cmd.push_str(&format!("--consensus.{} {} ", key, v));
        }
    }

    if is_bootstrap {
        cmd.push_str(&format!(
            "--network.secret_key {} 2>&1 | sed -r 's/\\x1B\\[[0-9;]*[mK]//g' > {}/validator{}.txt",
            SECRET_KEY, logs_dir, i
        ));
    } else {
        cmd.push_str(&format!(
            "--network.bootstrap_peer_multiaddr.#is_none false --network.bootstrap_peer_multiaddr \
             /ip4/127.0.0.1/tcp/{}/p2p/{} 2>&1 | sed -r 's/\\x1B\\[[0-9;]*[mK]//g' > \
             {}/validator{}.txt",
            *BOOTNODE_TCP_PORT, BOOT_NODE_PEER_ID, logs_dir, i
        ));
    }

    Node::new(i, monitoring_gateway_server_port, cmd)
}

async fn build_all_nodes(data_dir: &str, logs_dir: &str, papyrus_args: &PapyrusArgs) -> Vec<Node> {
    // Validators are started in a specific order to ensure proper network formation:
    // 1. The bootnode (validator 1) is started first for network peering.
    // 2. Validators 2+ are started next to join the network through the bootnode.
    // 3. Validator 0, which is the proposer, is started last so the validators don't miss the
    //    proposals.

    let mut nodes = Vec::new();

    nodes.push(build_node(data_dir, logs_dir, 1, papyrus_args).await); // Bootstrap 

    for i in 2..papyrus_args.num_validators {
        nodes.push(build_node(data_dir, logs_dir, i, papyrus_args).await);
    }

    nodes.push(build_node(data_dir, logs_dir, 0, papyrus_args).await); // Proposer

    nodes
}

#[tokio::main]
async fn main() {
    let Cli { papyrus_args, run_consensus_args } = Cli::parse();
    assert!(
        papyrus_args.num_validators >= 2,
        "At least 2 validators are required for the simulation."
    );

    let now_ns = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let mut tmpdir = std::env::temp_dir();
    tmpdir.push(format!("run_consensus_{now_ns}"));

    let logs_dir = tmpdir.to_str().unwrap().to_string();
    let db_dir = papyrus_args.db_dir.clone().unwrap_or_else(|| logs_dir.clone());
    fs::create_dir(&logs_dir).unwrap();

    if db_dir != logs_dir {
        let actual_dirs = fs::read_dir(&db_dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|d| d.file_type().unwrap().is_dir())
            .map(|d| d.file_name().into_string().unwrap())
            .collect();
        let expected_dirs: HashSet<_> =
            (0..papyrus_args.num_validators).map(|i| format!("data{}", i)).collect();
        assert!(expected_dirs.is_subset(&actual_dirs), "{db_dir} must contain: {expected_dirs:?}");
    } else {
        for i in 0..papyrus_args.num_validators {
            fs::create_dir_all(format!("{}/data{}", db_dir, i)).unwrap();
        }
    }

    // Acquire lock on the db_dir
    let _lock = LockDir::new(&db_dir).unwrap();

    println!("Running cargo build...");
    Command::new("cargo")
        .args(["build", "--release", "--package", "papyrus_node"])
        .status()
        .unwrap();

    println!("DB files will be stored in: {db_dir}");
    println!("Logs will be stored in: {logs_dir}");

    let nodes = build_all_nodes(&db_dir, &logs_dir, &papyrus_args).await;

    println!("Running validators...");
    run_simulation(
        nodes,
        run_consensus_args.max_test_duration,
        run_consensus_args.stagnation_threshold,
    )
    .await;

    println!("DB files were stored in: {}", db_dir);
    println!("Logs were stored in: {}", logs_dir);
    println!("Simulation complete.");
}
