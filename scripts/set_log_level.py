import argparse
import sys
from typing import List
import requests
import subprocess, signal, socket


def parse_args(args: List[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Set the log level for a crate")
    parser.add_argument(
        "--crate_name", type=str, help="The name of the crate to set the log level for"
    )
    parser.add_argument(
        "--log_level", type=str, help="The log level to set for the crate"
    )
    parser.add_argument(
        "--pod_name",
        type=str,
        default="",
        help="Optional Kubernetes pod name to port-forward to",
    )

    parser.add_argument(
        "--local_port",
        type=int,
        default=8082,
        help="Local port to bind the port-forward to (defaults to 8082)",
    )

    parser.add_argument(
        "--monitoring_port",
        type=int,
        default=8082,
        help="Monitoring port exposed by the pod (defaults to 8082)",
    )

    parser.add_argument(
        "--method",
        type=str,
        choices=["get", "post"],
        default="post",
        help="HTTP method to use: 'get' to read current log level, 'post' to set a log level",
    )
    return parser.parse_args(args)


def main():

    args = parse_args(sys.argv[1:])

    # If a pod name is supplied, establish a port-forward before making the request
    port_forward_proc = None

    target_port = args.monitoring_port
    base_port = args.local_port if args.pod_name else target_port

    if args.pod_name:
        cmd = [
            "kubectl",
            "port-forward",
            args.pod_name,
            f"{args.local_port}:{args.monitoring_port}",
        ]

        print("Starting port-forward:", " ".join(cmd))
        port_forward_proc = subprocess.Popen(
            cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE
        )

        try:
            with socket.create_connection(("localhost", args.local_port), timeout=1):
                print(
                    f"Port-forward to {args.pod_name}:{args.monitoring_port} is ready on localhost:{args.local_port}"
                )
        except OSError as e:
            print(
                f"Unexpected error: port-forward appears up but connection failed. Details: {e}"
            )

    try:
        if args.method == "get":
            full_url = f"http://localhost:{base_port}/monitoring/logLevel"
            print(f"Fetching current log level from {full_url}")
            response = requests.get(full_url, timeout=5)

            if response.status_code != 200:
                print(
                    f"Failed to fetch log level: {response.status_code} {response.text}"
                )
                sys.exit(1)

            print("Current log level response:\n", response.text)
        elif args.method == "post":
            # Validate required arguments
            if not args.crate_name or not args.log_level:
                print("--crate_name and --log_level are required when --method=post")
                sys.exit(1)

            base_url = f"http://localhost:{base_port}/monitoring/setLogLevel"
            full_url = f"{base_url}/{args.crate_name}/{args.log_level}"

            print(
                f"Setting log level for {args.crate_name} to {args.log_level} at {full_url}"
            )

            response = requests.post(full_url, timeout=5)

            if response.status_code != 200:
                print(
                    f"Failed to set log level for {args.crate_name} to {args.log_level}: {response.text}"
                )
                sys.exit(1)

            print(
                f"Successfully set log level for {args.crate_name} to {args.log_level}"
            )
        else:
            print(f"Unsupported method {args.method}. Use 'get' or 'post'.")
            sys.exit(1)
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
