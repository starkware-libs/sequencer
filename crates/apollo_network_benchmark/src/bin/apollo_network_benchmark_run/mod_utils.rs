use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context};

use crate::args::STRESS_TEST_NAME;

pub fn make_timestamp() -> String {
    chrono::Local::now().format("%Y-%m-%d-%H-%M-%S").to_string()
}

pub fn project_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).expect("workspace root").to_path_buf()
}

// This invokes `sh -c` on formatted command strings, so any untrusted input reaching the
// callers (image tags, namespaces, etc.) is executed as shell. The run binary is a
// developer tool — callers must supply trusted arguments.
pub fn run_cmd(cmd: &str, hint: &str, may_fail: bool) -> anyhow::Result<()> {
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
                bail!(
                    "Command failed with exit code {:?}: {}\n ⚠️ ⚠️ ⚠️  Hint: {}",
                    output.status.code(),
                    cmd,
                    hint
                );
            }
            Ok(())
        }
        Err(e) => {
            if may_fail {
                Ok(())
            } else {
                bail!("Failed to execute command: {}\nError: {}\nHint: {}", cmd, e, hint);
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

pub fn connect_to_cluster() -> anyhow::Result<()> {
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

pub fn get_commit() -> anyhow::Result<String> {
    let project_root = project_root();

    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&project_root)
        .output()
        .context("Failed to get git commit")?;

    if !output.status.success() {
        bail!("Failed to get git commit hash");
    }

    let mut commit_hash = String::from_utf8_lossy(&output.stdout).trim().to_string();

    let dirty_check = Command::new("git")
        .args(["diff-index", "--quiet", "HEAD"])
        .current_dir(&project_root)
        .status()
        .context("Failed to check git dirty status")?;

    if !dirty_check.success() {
        commit_hash.push_str("-dirty");
    }

    Ok(commit_hash)
}

const DEPLOYMENT_FILE_NAME: &str = "apollo_network_benchmark_deployment_file.json";

fn deployment_working_directory(prefix: &str) -> PathBuf {
    let home = std::env::var("HOME").expect("HOME environment variable not set");
    PathBuf::from(home).join(format!("apollo_network_benchmark_{}", prefix))
}

fn deployment_file_path(prefix: &str) -> PathBuf {
    deployment_working_directory(prefix).join(DEPLOYMENT_FILE_NAME)
}

pub fn local_deployment_working_directory() -> PathBuf {
    deployment_working_directory("local")
}

pub fn cluster_deployment_working_directory() -> PathBuf {
    deployment_working_directory("cluster")
}

pub fn cluster_deployment_file_path() -> PathBuf {
    deployment_file_path("cluster")
}

pub fn write_deployment_file(
    file_path: &Path,
    deployment_data: &serde_json::Value,
) -> anyhow::Result<()> {
    let content = serde_json::to_string_pretty(deployment_data)
        .context("Failed to serialize deployment data")?;
    fs::write(file_path, content).context("Failed to write deployment file")?;
    Ok(())
}

pub fn read_deployment_file(file_path: &Path) -> anyhow::Result<serde_json::Value> {
    anyhow::ensure!(
        file_path.exists(),
        "Deployment file does not exist at {}. Have you started a network stress test?",
        file_path.display()
    );
    let content = fs::read_to_string(file_path).context("Failed to read deployment file")?;
    serde_json::from_str(&content).context("Failed to parse deployment file")
}

pub fn read_cluster_deployment() -> anyhow::Result<serde_json::Value> {
    read_deployment_file(&cluster_deployment_file_path())
}

pub fn get_deployment_namespace(data: &serde_json::Value) -> anyhow::Result<&str> {
    data.get("namespace").and_then(|n| n.as_str()).context("No namespace found in deployment file")
}

pub fn get_deployment_num_nodes(data: &serde_json::Value) -> anyhow::Result<u64> {
    data.get("args")
        .and_then(|a| a.get("shared"))
        .and_then(|s| s.get("num_nodes"))
        .and_then(|n| n.as_u64())
        .context("No num_nodes found in deployment file")
}

pub fn write_json_file_to_working_dir(file_name: &str, file_content: &str) -> anyhow::Result<()> {
    let dir = cluster_deployment_working_directory();
    let file_path = dir.join(file_name);
    fs::write(&file_path, file_content)
        .with_context(|| format!("Failed to write file {}", file_name))?;
    Ok(())
}

pub fn make_cluster_image_tag(timestamp: &str) -> String {
    format!(
        "us-central1-docker.pkg.dev/starkware-dev/sequencer/{STRESS_TEST_NAME}-node:{timestamp}",
    )
}

pub fn make_local_image_tag(timestamp: &str) -> String {
    format!("{STRESS_TEST_NAME}-node:{timestamp}")
}

pub fn build_docker_image(image_tag: &str, fast_build: bool) -> anyhow::Result<()> {
    let project = project_root();

    if fast_build {
        pr!("Compiling apollo_network_benchmark_node without Docker...");
        run_cmd(
            r#"RUSTFLAGS="--cfg tokio_unstable" cargo build --release --bin apollo_network_benchmark_node"#,
            "Make sure you have Rust and Cargo installed.",
            false,
        )?;

        pr!("Copying binary to /tmp for Docker build...");
        let binary_path = project.join("target/release/apollo_network_benchmark_node");
        fs::copy(&binary_path, "/tmp/apollo_network_benchmark_node")
            .context("Failed to copy binary")?;

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

pub fn verify_docker_image_exists(image_tag: &str) -> anyhow::Result<()> {
    run_cmd(
        &format!("docker image inspect {} > /dev/null", image_tag),
        "Make sure the image exists before proceeding.",
        false,
    )
}

pub fn login_to_docker_registry() -> anyhow::Result<()> {
    run_cmd("gcloud auth configure-docker us-central1-docker.pkg.dev", "none", false)
}

pub fn upload_image_to_registry(image_tag: &str) -> anyhow::Result<()> {
    run_cmd(
        &format!("docker push {}", image_tag),
        "Make sure you are logged in to the Docker registry. If so, contact the dev team to \
         resolve any issues (maybe a permissions issue).",
        false,
    )
}

pub fn create_namespace(namespace_name: &str) -> anyhow::Result<()> {
    pr!("Creating namespace {}", namespace_name);
    run_cmd(&format!("kubectl create namespace {}", namespace_name), "none", false)
}

pub fn delete_namespace(namespace_name: &str, may_fail: bool) -> anyhow::Result<()> {
    run_cmd(&format!("kubectl delete namespace {}", namespace_name), "none", may_fail)
}

pub fn deploy_json_files_to_cluster(
    namespace_name: &str,
    file_names: &[String],
) -> anyhow::Result<()> {
    let dir = cluster_deployment_working_directory();

    let file_args: Vec<String> =
        file_names.iter().map(|name| format!("-f {}", dir.join(name).display())).collect();

    pr!("Deploying {} files to cluster", file_names.len());
    run_cmd(
        &format!("kubectl apply --wait {} -n {}", file_args.join(" "), namespace_name),
        "none",
        false,
    )
}

pub fn remove_directory(dir_path: &Path, may_fail: bool) -> anyhow::Result<()> {
    if dir_path.exists() {
        let result = fs::remove_dir_all(dir_path);
        if let Err(e) = result {
            if !may_fail {
                return Err(e)
                    .with_context(|| format!("Failed to remove directory {}", dir_path.display()));
            }
        }
    }
    Ok(())
}

pub fn run_with_deployment_guard(
    deployment_file: &Path,
    stop_fn: impl FnOnce() -> anyhow::Result<()>,
    run_fn: impl FnOnce() -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    if deployment_file.exists() {
        println!("Deployment file already exists. Do you want to stop the last run? (y/N): ");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).context("Failed to read input from stdin")?;

        if input.trim().to_lowercase() == "y" {
            stop_fn()?;
        } else {
            pr!("Exiting without running the experiment.");
            return Ok(());
        }
    }

    anyhow::ensure!(
        !deployment_file.exists(),
        "Deployment file still exists. Please run the stop command before starting a new run."
    );

    run_fn()
}
