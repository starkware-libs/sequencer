import argparse
import subprocess
import sys
import time
from typing import List

import requests
import signal
import socket

SLEEP_INTERVAL = 0.4


def parse_args(args: List[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Set the log level for a module or crate",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )

    # Add port-forwarding arguments
    add_port_forward_args(parser)

    parser.add_argument(
        "--target",
        type=str,
        help="Crate or module name whose log level should be inspected or updated",
    )
    parser.add_argument("--log_level", type=str, help="The log level to set for the crate")
    parser.add_argument(
        "--method",
        type=str,
        choices=["get", "post"],
        default="post",
        help="HTTP method to use: 'get' to read current log level, 'post' to set a log level",
    )
    return parser.parse_args(args)


def add_port_forward_args(parser: argparse.ArgumentParser) -> None:
    """Add port-forwarding related CLI options to the parser."""

    pf_group = parser.add_argument_group("port-forwarding options")

    pf_group.add_argument(
        "--pod_name",
        type=str,
        default="",
        help="Pod to port-forward to; omit when no port forwarding is needed",
    )

    pf_group.add_argument(
        "--local_port",
        type=int,
        default=8082,
        help="Local port to bind the port-forward to",
    )

    pf_group.add_argument(
        "--monitoring_port",
        type=int,
        default=8082,
        help="Monitoring endpoint port",
    )


def port_forward(
    pod_name: str,
    local_port: int,
    remote_port: int,
    max_attempts: int = 5,
) -> subprocess.Popen:
    """Start a kubectl port-forward and wait until it is ready.

    Returns the Popen handle so the caller can terminate it later.
    Raises RuntimeError if the local port is still unreachable after
    `max_attempts` connection checks.
    """

    cmd = ["kubectl", "port-forward", pod_name, f"{local_port}:{remote_port}"]
    print("Starting port-forward:", " ".join(cmd))
    proc = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

    for _attempt in range(1, max_attempts + 1):
        try:
            with socket.create_connection(("localhost", local_port), timeout=1):
                print(
                    f"✅ Port-forward to {pod_name}:{remote_port} is ready on localhost:{local_port}"
                )
                return proc
        except OSError:
            time.sleep(SLEEP_INTERVAL)

    proc.terminate()
    proc.wait(timeout=5)
    raise RuntimeError(
        f"❌ Port-forward to {pod_name}:{remote_port} failed after {max_attempts} attempts."
    )


def main():
    args = parse_args(sys.argv[1:])

    # If a pod name is supplied, establish a port-forward before making the request
    port_forward_proc = None

    target_port = args.monitoring_port
    base_port = args.local_port if args.pod_name else target_port

    if args.pod_name:
        try:
            port_forward_proc = port_forward(args.pod_name, args.local_port, args.monitoring_port)
        except RuntimeError as err:
            print(err)
            sys.exit(1)

    try:
        if args.method == "get":
            full_url = f"http://localhost:{base_port}/monitoring/logLevel"
            print(f"Fetching current log level from {full_url}")
            response = requests.get(full_url, timeout=5)

            if response.status_code != 200:
                print(f"Failed to fetch log level: {response.status_code} {response.text}")
                sys.exit(1)

            print("Current log level response:\n", response.text)
        elif args.method == "post":
            # Validate required arguments
            if not args.target or not args.log_level:
                print("--target and --log_level are required when --method=post")
                sys.exit(1)

            base_url = f"http://localhost:{base_port}/monitoring/setLogLevel"
            full_url = f"{base_url}/{args.target}/{args.log_level}"

            print(f"Setting log level for {args.target} to {args.log_level} at {full_url}")

            response = requests.post(full_url, timeout=5)

            if response.status_code != 200:
                print(
                    f"❌ Failed to set log level for {args.target} to {args.log_level}: {response.text}"
                )
                sys.exit(1)

            print(f"✅ Successfully set log level for {args.target} to {args.log_level}")

    finally:
        # Clean up the port-forward process if we started one
        if port_forward_proc:
            port_forward_proc.send_signal(signal.SIGINT)
            try:
                port_forward_proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                port_forward_proc.kill()
                port_forward_proc.wait()


if __name__ == "__main__":
    main()
