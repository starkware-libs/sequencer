use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn make_timestamp() -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).expect("Time went backwards");

    let datetime = chrono::DateTime::<chrono::Local>::from(
        UNIX_EPOCH + std::time::Duration::from_secs(now.as_secs()),
    );
    datetime.format("%Y-%m-%d-%H-%M-%S").to_string()
}

pub fn project_root() -> PathBuf {
    if let Ok(current_dir) = std::env::current_dir() {
        let mut path = current_dir.clone();
        loop {
            if path.ends_with("sequencer") {
                return path;
            }
            if !path.pop() {
                break;
            }
        }
    }
    panic!("Could not find project root (sequencer directory)");
}

pub fn run_cmd(cmd: &str, hint: &str, may_fail: bool) -> Result<(), String> {
    println!("🔧🔧🔧 CMD: {}", cmd);

    let output = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output();

    match output {
        Ok(output) => {
            if !output.status.success() && !may_fail {
                return Err(format!(
                    "Command failed with exit code {:?}: {}\n ⚠️ ⚠️ ⚠️  Hint: {}",
                    output.status.code(),
                    cmd,
                    hint
                ));
            }
            Ok(())
        }
        Err(e) => {
            if may_fail {
                Ok(())
            } else {
                Err(format!("Failed to execute command: {}\nError: {}\nHint: {}", cmd, e, hint))
            }
        }
    }
}

/// Macro to print informational messages with a bell icon prefix
#[macro_export]
macro_rules! pr {
    ($($arg:tt)*) => {
        println!("🔔 INFO: {}", format!($($arg)*))
    };
}

pub fn connect_to_cluster() -> Result<(), String> {
    run_cmd(
        "gcloud container clusters get-credentials sequencer-dev --region us-central1 --project \
         starkware-dev",
        "Make sure you have gcloud installed and you are logged in (run `gcloud auth login`).",
        false,
    )
}

pub fn make_multi_address(network_address: &str, port: u16, peer_id: &str, quic: bool) -> String {
    if quic {
        format!("{}/udp/{}/quic-v1/p2p/{}", network_address, port, peer_id)
    } else {
        format!("{}/tcp/{}/p2p/{}", network_address, port, peer_id)
    }
}

fn get_peer_id_from_secret_key(secret_key: &str) -> Result<String, String> {
    let project_root = project_root();
    let output = Command::new("cargo")
        .args(["run", "--bin", "get_peer_id_from_secret_key", secret_key])
        .current_dir(project_root)
        .output()
        .map_err(|e| format!("Failed to run get_peer_id_from_secret_key: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "Failed to get peer ID from secret key {}: {}",
            secret_key,
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.trim().replace("Peer ID: ", ""))
}

pub fn get_peer_id_from_node_id(node_id: u32) -> Result<String, String> {
    // Convert node ID to a 64-character hex string (32 bytes) with leading zeros
    let mut bytes = [0u8; 32];
    bytes[..4].copy_from_slice(&node_id.to_le_bytes());
    let secret_key = format!("0x{}", hex::encode(bytes));

    get_peer_id_from_secret_key(&secret_key)
}

pub fn get_commit() -> Result<String, String> {
    let project_root = project_root();

    // Get the commit hash
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&project_root)
        .output()
        .map_err(|e| format!("Failed to get git commit: {}", e))?;

    if !output.status.success() {
        return Err("Failed to get git commit hash".to_string());
    }

    let mut commit_hash = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // Check if there are uncommitted changes
    let dirty_check = Command::new("git")
        .args(["diff-index", "--quiet", "HEAD"])
        .current_dir(&project_root)
        .status()
        .map_err(|e| format!("Failed to check git dirty status: {}", e))?;

    // diff-index returns non-zero if there are changes
    if !dirty_check.success() {
        commit_hash.push_str("-dirty");
    }

    Ok(commit_hash)
}

// ============================================================================
// Deployment File Management
// ============================================================================

/// Deployment file name (same for both local and cluster)
const DEPLOYMENT_FILE_NAME: &str = "broadcast_network_stress_test_deployment_file.json";

/// Helper function to get deployment working directory with specified prefix
fn deployment_working_directory(prefix: &str) -> PathBuf {
    let home = std::env::var("HOME").expect("HOME environment variable not set");
    PathBuf::from(home).join(format!("{}_apollo_broadcast_network_stress_test", prefix))
}

/// Helper function to get deployment file path with specified prefix
fn deployment_file_path(prefix: &str) -> PathBuf {
    deployment_working_directory(prefix).join(DEPLOYMENT_FILE_NAME)
}

/// Returns the working directory for local deployments
pub fn local_deployment_working_directory() -> PathBuf {
    deployment_working_directory("local")
}

/// Returns the working directory for cluster deployments
pub fn cluster_deployment_working_directory() -> PathBuf {
    deployment_working_directory("cluster")
}

/// Returns the deployment file path for local deployments
pub fn local_deployment_file_path() -> PathBuf {
    deployment_file_path("local")
}

/// Returns the deployment file path for cluster deployments
pub fn cluster_deployment_file_path() -> PathBuf {
    deployment_file_path("cluster")
}

/// Writes deployment data to a JSON file
pub fn write_deployment_file(
    file_path: &Path,
    deployment_data: &serde_json::Value,
) -> Result<(), String> {
    let content = serde_json::to_string_pretty(deployment_data)
        .map_err(|e| format!("Failed to serialize deployment data: {}", e))?;

    fs::write(file_path, content).map_err(|e| format!("Failed to write deployment file: {}", e))?;

    Ok(())
}

/// Reads and parses deployment data from a JSON file
pub fn read_deployment_file(file_path: &Path) -> Result<serde_json::Value, String> {
    if !file_path.exists() {
        return Err(format!(
            "Deployment file does not exist at {}. Have you started a network stress test?",
            file_path.display()
        ));
    }

    let content = fs::read_to_string(file_path)
        .map_err(|e| format!("Failed to read deployment file: {}", e))?;

    serde_json::from_str(&content).map_err(|e| format!("Failed to parse deployment file: {}", e))
}

/// Writes a JSON file to the cluster deployment working directory
pub fn write_json_file_to_working_dir(file_name: &str, file_content: &str) -> Result<(), String> {
    let dir = cluster_deployment_working_directory();
    let file_path = dir.join(file_name);

    fs::write(&file_path, file_content)
        .map_err(|e| format!("Failed to write file {}: {}", file_name, e))?;

    Ok(())
}

// ============================================================================
// Docker Operations
// ============================================================================

/// Creates a Docker image tag for cluster deployments
pub fn make_cluster_image_tag(timestamp: &str) -> String {
    format!(
        "us-central1-docker.pkg.dev/starkware-dev/sequencer/broadcast-network-stress-test-node:{}",
        timestamp
    )
}

/// Creates a Docker image tag for local deployments
pub fn make_local_image_tag(timestamp: &str) -> String {
    format!("broadcast-network-stress-test-node:{}", timestamp)
}

/// Builds a Docker image using either fast or slow build method
pub fn build_docker_image(image_tag: &str, fast_build: bool) -> Result<(), String> {
    let project = project_root();

    if fast_build {
        pr!("Compiling broadcast_network_stress_test_node node without Docker...");
        run_cmd(
            r#"RUSTFLAGS="--cfg tokio_unstable" cargo build --release --bin broadcast_network_stress_test_node"#,
            "Make sure you have Rust and Cargo installed.",
            false,
        )?;

        pr!("Copying binary to /tmp for Docker build...");
        let binary_path = project.join("target/release/broadcast_network_stress_test_node");
        run_cmd(&format!("cp {} /tmp/", binary_path.display()), "none", false)?;

        let dockerfile_path = project.join("crates/apollo_network_benchmark/run/Dockerfile.fast");
        run_cmd(
            &format!(
                "docker build -t {} -f {} --build-context tmp=/tmp {}",
                image_tag,
                dockerfile_path.display(),
                project.display()
            ),
            "none",
            false,
        )?;
    } else {
        let dockerfile_path = project.join("crates/apollo_network_benchmark/run/Dockerfile.slow");
        run_cmd(
            &format!(
                "docker build -t {} -f {} {}",
                image_tag,
                dockerfile_path.display(),
                project.display()
            ),
            "none",
            false,
        )?;
    }

    Ok(())
}

/// Verifies that a Docker image exists
pub fn verify_docker_image_exists(image_tag: &str) -> Result<(), String> {
    run_cmd(
        &format!("docker image inspect {} > /dev/null", image_tag),
        "Make sure the image exists before proceeding.",
        false,
    )
}

/// Logs in to the Docker registry for cluster deployments
pub fn login_to_docker_registry() -> Result<(), String> {
    run_cmd("gcloud auth configure-docker us-central1-docker.pkg.dev", "none", false)
}

/// Uploads a Docker image to the registry
pub fn upload_image_to_registry(image_tag: &str) -> Result<(), String> {
    run_cmd(
        &format!("docker push {}", image_tag),
        "Make sure you are logged in to the Docker registry. If so, contact the dev team to \
         resolve any issues (maybe a permissions issue).",
        false,
    )
}

/// Checks if Docker is working correctly
pub fn check_docker() -> Result<(), String> {
    pr!("Checking if Docker works...");
    run_cmd(
        "docker run --name hello-world hello-world",
        "Make sure you have Docker installed and running.",
        false,
    )?;
    run_cmd("docker rm hello-world", "none", false)?;
    pr!("Docker is working correctly.");
    Ok(())
}

// ============================================================================
// Kubernetes Operations
// ============================================================================

/// Creates a Kubernetes namespace
pub fn create_namespace(namespace_name: &str) -> Result<(), String> {
    pr!("Creating namespace {}", namespace_name);
    run_cmd(&format!("kubectl create namespace {}", namespace_name), "none", false)
}

/// Deletes a Kubernetes namespace
pub fn delete_namespace(namespace_name: &str, may_fail: bool) -> Result<(), String> {
    run_cmd(&format!("kubectl delete namespace {}", namespace_name), "none", may_fail)
}

/// Deploys JSON files to a Kubernetes namespace
pub fn deploy_json_files_to_cluster(
    namespace_name: &str,
    file_names: &[String],
) -> Result<(), String> {
    let dir = cluster_deployment_working_directory();

    for file_name in file_names {
        pr!("Deploying {} to cluster", file_name);
        let file_path = dir.join(file_name);
        run_cmd(
            &format!("kubectl apply --wait -f {} -n {}", file_path.display(), namespace_name),
            "none",
            false,
        )?;
    }

    Ok(())
}

// ============================================================================
// Docker Network Operations (Local Deployments)
// ============================================================================

/// Creates a Docker network for local deployments
pub fn create_docker_network(network_name: &str) -> Result<(), String> {
    pr!("Creating custom Docker network: {}", network_name);
    run_cmd(&format!("docker network create {}", network_name), "none", false)
}

/// Removes a Docker network
pub fn remove_docker_network(network_name: &str, may_fail: bool) -> Result<(), String> {
    pr!("Removing Docker network {}...", network_name);
    run_cmd(&format!("docker network rm {}", network_name), "none", may_fail)
}

/// Stops and removes a Docker container
pub fn remove_docker_container(container_name: &str, may_fail: bool) -> Result<(), String> {
    run_cmd(&format!("docker rm -f {}", container_name), "none", may_fail)
}

// ============================================================================
// Cleanup Operations
// ============================================================================

/// Removes a directory recursively
pub fn remove_directory(dir_path: &Path, may_fail: bool) -> Result<(), String> {
    if dir_path.exists() {
        run_cmd(&format!("rm -rf {}", dir_path.display()), "none", may_fail)?;
    }
    Ok(())
}
