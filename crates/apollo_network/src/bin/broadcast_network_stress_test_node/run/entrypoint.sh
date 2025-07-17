#!/bin/bash

set -e

echo "Starting container with hostname: $(hostname)"

# ******************************** time synchronization ********************************

# Start chrony for time synchronization
echo "Starting chrony for time synchronization..."
chronyd -d &
CHRONY_PID=$!

# Wait a moment for chrony to initialize
sleep 2

# Verify chrony is working
if ps -p $CHRONY_PID > /dev/null; then
    echo "Chrony started successfully (PID: $CHRONY_PID)"
else
    echo "Warning: Chrony failed to start"
fi

# ***************************** throttling connection start *****************************

INTERFACE="eth0"  # Default Docker interface

# Ports to throttle (egress only)
TARGET_PORT_TCP_EGRESS=$P2P_PORT  # The specific TCP port for egress throttling
TARGET_PORT_UDP_EGRESS=$P2P_PORT  # The specific UDP port for egress throttling

# Load ifb module is not strictly needed if only egress, but leaving for safety if you re-introduce ingress later.
# modprobe ifb || echo "ifb module already loaded or not needed"
# ip link add ifb0 type ifb || true
# ip link set ifb0 up || true
# tc qdisc add dev $INTERFACE ingress handle ffff: || true
# tc filter add dev $INTERFACE parent ffff: protocol ip u32 match u32 0 0 action mirred egress redirect dev ifb0 || true


# Function to apply shaping (htb for bandwidth + netem for latency) to a specific class
apply_shaping_to_class() {
  local dev=$1
  local parent_handle=$2 # e.g., 1:1 for a class
  local class_id=$3      # e.g., 10 for 1:10

  # If throughput is set, calculate rate in kbit/s (assuming THROUGHPUT in KB/s)
  if [ ! -z "${THROUGHPUT}" ]; then
    echo "Gating network throughput for class ${parent_handle} at ${THROUGHPUT} KB/s."
    RATE=$((THROUGHPUT * 8))
    # Note: The htb qdisc is added at a higher level (root or parent of the class)
    # The rate is applied to the class.
    tc class add dev $dev parent ${parent_handle} classid ${class_id} htb rate ${RATE}kbit ceil ${RATE}kbit || true
    netem_parent="${class_id}"
  else
    # If no throughput, netem applies directly to the parent class.
    # We still need a high default rate for the class itself if THROUGHPUT is not set,
    # otherwise, the class might implicitly throttle.
    tc class add dev $dev parent ${parent_handle} classid ${class_id} htb rate 1000mbit ceil 1000mbit || true
    netem_parent="${class_id}"
  fi

  # If latency is set, add netem (delay in ms)
  if [ ! -z "${LATENCY}" ]; then
    echo "Gating network latency for class ${parent_handle} at ${LATENCY} milliseconds."
    # If a class was created for throughput, apply netem to that class. Otherwise, to the parent.
    tc qdisc add dev $dev parent ${netem_parent} netem delay ${LATENCY}ms || true
  fi
}

# --- Egress (eth0) Throttling Setup ---

# 1. Add HTB root qdisc to eth0
# default 10 means traffic not matched by filters goes to class 1:10 (unthrottled)
tc qdisc add dev $INTERFACE root handle 1: htb default 10 || true

# 2. Define the 'unthrottled' class (1:10)
tc class add dev $INTERFACE parent 1: classid 1:10 htb rate 1000mbit ceil 1000mbit || true # High rate for unthrottled

# 3. Define the 'throttled' class (1:20) for both TCP and UDP
# This class will receive the THROUGHPUT and LATENCY settings
# We apply the shaping to this single class for both ports.
# The `rate` and `ceil` here are placeholders; they will be overridden by apply_shaping_to_class if THROUGHPUT is set.
tc class add dev $INTERFACE parent 1: classid 1:20 htb rate 1000mbit ceil 1000mbit || true

# 4. Apply the common shaping parameters (THROUGHPUT, LATENCY) to the throttled class (1:20)
apply_shaping_to_class $INTERFACE "1:20" "1:20"

# 5. Add filters to direct specific port traffic to the throttled class (1:20)

# Filter for TCP Egress
tc filter add dev $INTERFACE protocol ip parent 1: prio 1 u32 \
  match ip dport $TARGET_PORT_TCP_EGRESS 0xffff \
  match ip protocol 6 0xff \
  flowid 1:20 || true
echo "Throttling TCP egress on port ${TARGET_PORT_TCP_EGRESS}"

# Filter for UDP Egress
tc filter add dev $INTERFACE protocol ip parent 1: prio 2 u32 \
  match ip dport $TARGET_PORT_UDP_EGRESS 0xffff \
  match ip protocol 17 0xff \
  flowid 1:20 || true
echo "Throttling UDP egress on port ${TARGET_PORT_UDP_EGRESS}"


# ***************************** throttling connection end *****************************

# Call broadcast_network_stress_test_node
export ID=$(hostname | grep -o '[0-9]*$')
echo "Starting node with ID: $ID"
exec broadcast_network_stress_test_node