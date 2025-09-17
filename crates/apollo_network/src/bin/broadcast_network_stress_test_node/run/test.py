#!/usr/bin/env python3
"""
Test script for network protocols: Gossipsub, SQMR, and ReveresedSqmr.

This script:
1. Runs the broadcast network stress test with different protocols
2. Waits 60 seconds for messages to be exchanged
3. Verifies network connections are established via network_connected_peers metric
4. Analyzes sent vs received message metrics to identify broadcaster vs receiver issues
5. Checks for successful message reception and provides detailed failure diagnostics
6. Cleans up the running process

Usage:
    cd /home/andrew/workspace/sequencer/crates/apollo_network/src/bin/broadcast_network_stress_test_node/run

    # Test all protocols
    python3 test.py

    # Test specific protocol
    python3 test.py --protocol gossipsub
    python3 test.py --protocol sqmr
    python3 test.py --protocol reversed-sqmr

The test supports three protocols:
- Gossipsub: Traditional pub/sub broadcasting
- SQMR: Query/response protocol where broadcaster sends queries
- ReveresedSqmr: Query/response protocol where receivers send queries
"""

import subprocess
import time
import requests
import sys
import signal
import os
import argparse


def cleanup_process(process):
    """Clean up a running process gracefully."""
    if not process:
        return

    print("Cleaning up process...")
    try:
        # Send SIGINT (Ctrl+C) to allow graceful cleanup of containers
        process.send_signal(signal.SIGINT)
        print("Sent SIGINT to process, waiting for graceful shutdown...")

        # Wait up to 30 seconds for graceful shutdown
        try:
            process.wait(timeout=30)
            print(f"Process {process.pid} terminated gracefully")
        except subprocess.TimeoutExpired:
            print("Process didn't terminate gracefully, sending SIGTERM...")
            process.terminate()
            try:
                process.wait(timeout=10)
                print(f"Process {process.pid} terminated with SIGTERM")
            except subprocess.TimeoutExpired:
                print("Process didn't respond to SIGTERM, sending SIGKILL...")
                process.kill()
                process.wait()
                print(f"Process {process.pid} killed with SIGKILL")

        # Print any output for debugging
        try:
            stdout, stderr = process.communicate(timeout=1)
            if stdout:
                print("Process stdout (last 1000 chars):")
                print(stdout.decode()[-1000:])
            if stderr:
                print("Process stderr (last 1000 chars):")
                print(stderr.decode()[-1000:])
        except subprocess.TimeoutExpired:
            pass

    except ProcessLookupError:
        print("Process already terminated")
    except Exception as cleanup_error:
        print(f"Error during cleanup: {cleanup_error}")


def check_network_connections():
    """Check if nodes have established network connections via Prometheus metrics."""
    try:
        # Query Prometheus for connected peers metric
        prometheus_url = "http://localhost:9090/api/v1/query"
        params = {"query": "network_connected_peers"}

        response = requests.get(prometheus_url, params=params, timeout=10)
        response.raise_for_status()
        data = response.json()

        if data.get("status") != "success":
            print(f"❌ Prometheus query failed: {data}")
            return False

        results = data.get("data", {}).get("result", [])

        if not results:
            print("❌ network_connected_peers metric not found in Prometheus")
            return False

        # Check connection metrics for each node
        total_connections = 0
        nodes_with_connections = 0

        for result in results:
            value = float(result.get("value", [0, "0"])[1])
            instance = result.get("metric", {}).get("instance", "unknown")
            print(f"Node {instance}: {value} connected peers")

            total_connections += value
            if value > 0:
                nodes_with_connections += 1

        # For a 2-node test, we expect at least 1 connection total (bidirectional connection)
        # and at least 1 node should have connections
        if total_connections >= 1 and nodes_with_connections >= 1:
            print(
                f"✅ PASS: Network connections established (total: {total_connections}, nodes with connections: {nodes_with_connections})"
            )
            return True
        else:
            print(
                f"❌ FAIL: Insufficient network connections (total: {total_connections}, nodes with connections: {nodes_with_connections})"
            )
            return False

    except requests.exceptions.RequestException as e:
        print(f"❌ Failed to query Prometheus for connections: {e}")
        return False
    except Exception as e:
        print(f"❌ Error checking network connections: {e}")
        return False


def check_broadcaster_vs_receiver():
    """
    Check if the issue is with broadcaster (not sending) or receiver (not receiving).

    Returns:
        tuple: (broadcaster_issue, receiver_issue) where True indicates an issue
    """
    try:
        prometheus_url = "http://localhost:9090/api/v1/query"

        # Query both sent and received messages metrics (actual metrics used by stress test)
        sent_params = {"query": "broadcast_message_count"}
        received_params = {"query": "receive_message_count"}

        # Get sent messages
        sent_response = requests.get(prometheus_url, params=sent_params, timeout=10)
        sent_response.raise_for_status()
        sent_data = sent_response.json()

        # Get received messages
        received_response = requests.get(
            prometheus_url, params=received_params, timeout=10
        )
        received_response.raise_for_status()
        received_data = received_response.json()

        # Parse sent messages
        total_sent = 0
        if sent_data.get("status") == "success":
            sent_results = sent_data.get("data", {}).get("result", [])
            for result in sent_results:
                value = float(result.get("value", [0, "0"])[1])
                instance = result.get("metric", {}).get("instance", "unknown")
                print(f"Node {instance}: {value} messages sent")
                total_sent += value

        # Parse received messages
        total_received = 0
        if received_data.get("status") == "success":
            received_results = received_data.get("data", {}).get("result", [])
            for result in received_results:
                value = float(result.get("value", [0, "0"])[1])
                instance = result.get("metric", {}).get("instance", "unknown")
                print(f"Node {instance}: {value} messages received")
                total_received += value

        print(
            f"📊 Summary: {total_sent} messages sent, {total_received} messages received"
        )

        # Determine issues
        broadcaster_issue = total_sent == 0  # No messages being sent
        receiver_issue = (
            total_sent > 0 and total_received == 0
        )  # Messages sent but none received

        if broadcaster_issue:
            print("❌ Broadcaster Issue: No messages are being sent")
        elif receiver_issue:
            print("❌ Receiver Issue: Messages are being sent but not received")
        elif total_sent > 0 and total_received > 0:
            print("✅ Both broadcaster and receiver are working")
        else:
            print("⚠️  No message activity detected")

        return broadcaster_issue, receiver_issue

    except requests.exceptions.RequestException as e:
        print(f"❌ Failed to query Prometheus for message metrics: {e}")
        return True, True  # Assume both have issues if we can't check
    except Exception as e:
        print(f"❌ Error checking broadcaster vs receiver: {e}")
        return True, True


def run_protocol_test(protocol_name):
    """Test a specific network protocol."""
    print(f"Starting {protocol_name} test...")

    # 1. Run the command in the background
    cmd = [
        "python3",
        "local.py",
        "--mode",
        "one",
        "--num-nodes",
        "2",
        "--network-protocol",
        protocol_name,
    ]

    print(f"Running command: {' '.join(cmd)}")
    process = subprocess.Popen(
        cmd,
        text=True,
        preexec_fn=os.setsid,
    )

    try:

        print(f"Process started with PID: {process.pid}")

        # 2. Sleep for 60 seconds
        print("Waiting 60 seconds for message exchange...")
        for i in range(60, 0, -10):
            print(f"  {i} seconds remaining...")
            time.sleep(10)
            if process.poll() is not None:
                raise Exception(
                    f"Process terminated early with return code {process.returncode}"
                )

        # 3. Query Prometheus for connection metrics first
        print("Checking if nodes have established connections...")

        connection_success = check_network_connections()
        if not connection_success:
            raise Exception("Nodes failed to establish connections")

        # 4. Check broadcaster vs receiver issue for detailed diagnostics
        print("Checking if the issue is in broadcaster or receiver...")

        broadcaster_issue, receiver_issue = check_broadcaster_vs_receiver()

        # 5. Query Prometheus for the message metric
        print("Querying Prometheus for receive_message_count metric...")

        # Query Prometheus API
        prometheus_url = "http://localhost:9090/api/v1/query"
        params = {"query": "receive_message_count"}

        try:
            response = requests.get(prometheus_url, params=params, timeout=10)
            response.raise_for_status()
        except requests.exceptions.RequestException as e:
            raise Exception(
                f"Failed to query Prometheus: {e}. Make sure Prometheus is running on localhost:9090"
            )

        try:
            data = response.json()
        except Exception as e:
            raise Exception(f"Error parsing Prometheus response: {e}")

        if data.get("status") != "success":
            raise Exception(f"Prometheus query failed: {data}")

        results = data.get("data", {}).get("result", [])

        if not results:
            raise Exception("receive_message_count metric not found in Prometheus")

        # Check if any of the metric values is > 0
        total_messages = 0
        for result in results:
            value = float(result.get("value", [0, "0"])[1])
            total_messages += value
            print(
                f"Found metric value: {value} for instance {result.get('metric', {})}"
            )

        if total_messages <= 0:
            # Provide more detailed error message based on broadcaster/receiver analysis
            if broadcaster_issue and receiver_issue:
                raise Exception(
                    "Both broadcaster and receiver have issues - no messages sent or received"
                )
            elif broadcaster_issue:
                raise Exception(
                    "Broadcaster issue: No messages being sent by the broadcaster node"
                )
            elif receiver_issue:
                raise Exception(
                    "Receiver issue: Messages are being sent but not received by receiver nodes"
                )
            else:
                raise Exception(f"receive_message_count = {total_messages} (not > 0)")

        print(f"✅ PASS: receive_message_count = {total_messages} (> 0)")
        return True

    except Exception as e:
        print(f"❌ FAIL: {e}")

        # Get process output on any failure if not already captured
        if process:
            try:
                stdout, stderr = process.communicate(timeout=1)
                print(f"Process stdout:\n{stdout}")
                print(f"Process stderr:\n{stderr}")
            except:
                pass

        return False

    finally:
        # 5. Clean up the running process
        cleanup_process(process)


def run_all_tests():
    """Run tests for all network protocols."""
    protocols = ["gossipsub", "sqmr", "reversed-sqmr"]
    results = {}

    for protocol in protocols:
        print("=" * 80)
        print(f"Testing {protocol.upper()} Protocol")
        print("=" * 80)

        success = run_protocol_test(protocol)
        results[protocol] = success

        print("=" * 80)
        if success:
            print(f"✅ {protocol.upper()} TEST PASSED!")
        else:
            print(f"❌ {protocol.upper()} TEST FAILED!")
        print("=" * 80)
        print()

        # Wait a bit between tests to ensure clean separation
        if protocol != protocols[-1]:  # Don't wait after the last test
            print("Waiting 10 seconds before next test...")
            time.sleep(10)

    return results


def main():
    """Entry point."""
    parser = argparse.ArgumentParser(description="Network Protocol Test Suite")
    parser.add_argument(
        "--protocol",
        choices=["gossipsub", "sqmr", "reversed-sqmr", "all"],
        default="all",
        help="Protocol to test (default: all)",
    )
    args = parser.parse_args()

    if args.protocol == "all":
        print("=" * 80)
        print("Network Protocol Comparison Test")
        print("Testing: Gossipsub, SQMR, and ReveresedSqmr")
        print("=" * 80)
        print()

        results = run_all_tests()

        print("=" * 80)
        print("FINAL RESULTS:")
        print("=" * 80)

        all_passed = True
        for protocol, success in results.items():
            status = "✅ PASSED" if success else "❌ FAILED"
            print(f"{protocol.upper():15} : {status}")
            if not success:
                all_passed = False

        print("=" * 80)
        if all_passed:
            print("🎉 ALL TESTS PASSED!")
            sys.exit(0)
        else:
            print("💥 SOME TESTS FAILED!")
            sys.exit(1)
    else:
        # Run single protocol test
        print("=" * 80)
        print(f"Testing {args.protocol.upper()} Protocol")
        print("=" * 80)

        success = run_protocol_test(args.protocol)

        print("=" * 80)
        if success:
            print(f"🎉 {args.protocol.upper()} TEST PASSED!")
            sys.exit(0)
        else:
            print(f"💥 {args.protocol.upper()} TEST FAILED!")
            sys.exit(1)


if __name__ == "__main__":
    main()
