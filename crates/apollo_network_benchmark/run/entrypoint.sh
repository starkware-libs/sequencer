#!/bin/bash

set -e

echo "Starting container with hostname: $(hostname)"

# For Indexed Jobs: Set stable hostname based on JOB_COMPLETION_INDEX
# This enables stable DNS names like: broadcast-network-stress-test-0.broadcast-network-stress-test-headless
if [ ! -z "$JOB_COMPLETION_INDEX" ]; then
    NEW_HOSTNAME="broadcast-network-stress-test-${JOB_COMPLETION_INDEX}"
    hostname "$NEW_HOSTNAME" || echo "Warning: Could not set hostname to $NEW_HOSTNAME"
    echo "Set hostname to: $(hostname) (based on JOB_COMPLETION_INDEX=$JOB_COMPLETION_INDEX)"
fi

# ********************************* machine information *********************************

echo "Machine identification:"
echo "  Container ID: $(cat /proc/self/cgroup 2>/dev/null | head -1 | cut -d/ -f3 | cut -c1-12 || echo 'N/A')"
echo "  Host IP addresses:"
ip addr show | grep -E 'inet [0-9]' | awk '{print "    " $2}' || echo "    IP info unavailable"
echo "  Machine ID: $(cat /etc/machine-id 2>/dev/null || echo 'N/A')"
echo "  Kernel: $(uname -r)"
echo "  Architecture: $(uname -m)"
if [ -n "$NODE_NAME" ]; then
    echo "  Kubernetes Node: $NODE_NAME"
fi
if [ -n "$KUBERNETES_NODE_NAME" ]; then
    echo "  K8s Node Name: $KUBERNETES_NODE_NAME"
fi

# ***************************** throttling connection start *****************************

set -e

INTERFACE="eth0"  # Default Docker interface

# Load ifb module for ingress shaping
modprobe ifb || echo "ifb module already loaded or not needed"

# Set up ifb0 for ingress
ip link add ifb0 type ifb || true
ip link set ifb0 up || true

# Redirect all ingress traffic to ifb0
tc qdisc add dev $INTERFACE ingress handle ffff: || true
tc filter add dev $INTERFACE parent ffff: protocol ip u32 match u32 0 0 action mirred egress redirect dev ifb0 || true

# Function to apply shaping (htb for bandwidth + netem for latency)
apply_shaping() {
  local dev=$1
  local parent=$2
  local handle=$3  # e.g., 1 (no trailing :)

  # If throughput is set, calculate rate in kbit/s (assuming THROUGHPUT in KB/s)
  if [ ! -z "${THROUGHPUT}" ]; then
    RATE=$((THROUGHPUT * 8))
    tc qdisc add dev $dev $parent handle ${handle}: htb default 1 || true
    tc class add dev $dev parent ${handle}: classid ${handle}:1 htb rate ${RATE}kbit ceil ${RATE}kbit || true
    netem_parent="${handle}:1"
  else
    netem_parent="root"
  fi

  # If latency is set, add netem (delay in ms)
  # Calculate queue limit to prevent packet drops with large delays
  # Formula: limit = (expected_throughput_bps * delay_seconds) / (avg_packet_size_bytes * 8)
  # For safety, we use a generous default: assume 1500 byte packets and calculate for 1 second of buffering
  # This gives us: (throughput_kbps * 1000 * delay_ms / 1000) / (1500 * 8) = (throughput_kbps * delay_ms) / 12000
  if [ ! -z "${LATENCY}" ]; then
    if [ ! -z "${THROUGHPUT}" ]; then
      # Calculate limit based on throughput and latency
      # Throughput is in KB/s, latency in ms
      # Buffer for at least 2x the delay to handle bursts
      PACKET_SIZE=1500  # bytes
      BUFFER_MULTIPLIER=2
      LIMIT=$(( (THROUGHPUT * 1000 * LATENCY * BUFFER_MULTIPLIER) / (PACKET_SIZE * 8) ))
      # Ensure minimum limit of 1000 packets
      if [ $LIMIT -lt 1000 ]; then
        LIMIT=1000
      fi
      # Cap at reasonable maximum (100k packets) to avoid excessive memory usage
      if [ $LIMIT -gt 100000 ]; then
        LIMIT=100000
      fi
      tc qdisc add dev $dev parent $netem_parent netem delay ${LATENCY}ms limit ${LIMIT} || true
      echo "Applied netem delay ${LATENCY}ms with queue limit ${LIMIT} packets"
    else
      # No throughput limit, use a generous default based on latency alone
      # Assume we might need to buffer packets arriving at 100 Mbps for the delay duration
      PACKET_SIZE=1500
      ASSUMED_RATE_MBPS=100
      BUFFER_MULTIPLIER=2
      LIMIT=$(( (ASSUMED_RATE_MBPS * 1000 * 1000 * LATENCY * BUFFER_MULTIPLIER) / (1000 * PACKET_SIZE * 8) ))
      if [ $LIMIT -lt 1000 ]; then
        LIMIT=1000
      fi
      if [ $LIMIT -gt 100000 ]; then
        LIMIT=100000
      fi
      tc qdisc add dev $dev parent $netem_parent netem delay ${LATENCY}ms limit ${LIMIT} || true
      echo "Applied netem delay ${LATENCY}ms with queue limit ${LIMIT} packets (default calculation)"
    fi
  fi
}

# Apply to egress (eth0)
# apply_shaping $INTERFACE "root" "1"

# Apply to ingress (ifb0)
apply_shaping ifb0 "root" "1"

# ***************************** throttling connection end *****************************

# Call broadcast_network_stress_test_node
# Use ID from environment variable if set, otherwise use JOB_COMPLETION_INDEX (for Indexed Jobs)
if [ -z "$ID" ]; then
    if [ ! -z "$JOB_COMPLETION_INDEX" ]; then
        # Used when running in Indexed Job
        export ID=$JOB_COMPLETION_INDEX
        echo "ID not set in environment, using JOB_COMPLETION_INDEX: $ID"
    else
        # Fallback: extract from hostname (legacy StatefulSet support)
        export ID=$(hostname | grep -o '[0-9]*$')
        echo "ID not set in environment, extracted from hostname: $ID"
    fi
else
    # Used when running locally
    echo "Using ID from environment variable: $ID"
fi
exec broadcast_network_stress_test_node