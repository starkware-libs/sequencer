#!/bin/bash

set -e

echo "Starting container with hostname: $(hostname)"

# ***************************** throttling connection start *****************************

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
    echo "Gating network throughput at ${THROUGHPUT} KB/s."
    RATE=$((THROUGHPUT * 8))
    tc qdisc add dev $dev $parent handle ${handle}: htb default 1 || true
    tc class add dev $dev parent ${handle}: classid ${handle}:1 htb rate ${RATE}kbit ceil ${RATE}kbit || true
    netem_parent="${handle}:1"
  else
    netem_parent="root"
  fi

  # If latency is set, add netem (delay in ms)
  if [ ! -z "${LATENCY}" ]; then
    echo "Gating network latency at ${LATENCY} milliseconds."
    tc qdisc add dev $dev parent $netem_parent netem delay ${LATENCY}ms || true
  fi
}

# Apply to egress (eth0)
apply_shaping $INTERFACE "root" "1"

# Apply to ingress (ifb0)
apply_shaping ifb0 "root" "1"


# ***************************** throttling connection end *****************************

# Call broadcast_network_stress_test_node
export ID=$(hostname | grep -o '[0-9]*$')
echo "Starting node with ID: $ID"
exec broadcast_network_stress_test_node