import os
import time
import subprocess
import functools


def make_timestamp() -> str:
    return time.strftime("%Y-%m-%d-%H-%M-%S", time.localtime())


def project_root() -> str:
    result = os.path.abspath(f"{__file__}/../../../../../../../")
    assert result.endswith(
        "/sequencer"
    ), f"Project root must end in '/sequencer' but {result}"
    return result


def run_cmd(cmd: str, hint: str = "none", may_fail: bool = False):
    print(f"🔧🔧🔧 CMD: {cmd}", flush=True)
    result = os.system(cmd)
    if result != 0 and not may_fail:
        raise RuntimeError(
            f"Command failed with exit code {result}: {cmd}\n ⚠️ ⚠️ ⚠️  Hint: {hint}"
        )


def pr(string: str):
    print(f"🔔 INFO: {string}", flush=True)


def connect_to_cluster():
    run_cmd(
        "gcloud container clusters get-credentials sequencer-dev --region us-central1 --project starkware-dev",
        hint="Make sure you have gcloud installed and you are logged in (run `gcloud auth login`).",
    )


def make_multi_address(network_address: str, port: int, peer_id: str, args) -> str:
    if args.tcp:
        return f"{network_address}/tcp/{port}/p2p/{peer_id}"
    else:
        return f"{network_address}/udp/{port}/quic-v1/p2p/{peer_id}"


def __get_peer_id_from_secret_key(secret_key: str) -> str:
    """Get peer ID by running the cargo command with the given secret key."""
    cmd = f"cargo run --bin get_peer_id_from_secret_key {secret_key}"
    try:
        result = subprocess.run(
            cmd,
            shell=True,
            capture_output=True,
            text=True,
            check=True,
            cwd=project_root(),
        )
        return result.stdout.strip().replace("Peer ID: ", "")
    except subprocess.CalledProcessError as e:
        raise RuntimeError(f"Failed to get peer ID from secret key {secret_key}: {e}")


@functools.lru_cache(maxsize=128)
def get_peer_id_from_node_id(node_id: int) -> str:
    """Get peer ID for a node by converting node ID to secret key and running the cargo command."""
    # Convert node ID to a 64-character hex string (32 bytes) with leading zeros
    bytes = node_id.to_bytes(32, "little")
    secret_key = f"0x{bytes.hex()}"
    return __get_peer_id_from_secret_key(secret_key)
