#!/bin/bash

set -e

echo "Starting container with hostname: $(hostname)"

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

INTERFACE="eth0"  # Default Docker interface

# Ports to throttle (both directions)
TARGET_PORT_TCP=$P2P_PORT  # The specific TCP port for throttling
TARGET_PORT_UDP=$P2P_PORT  # The specific UDP port for throttling

# --- ADDED: Cleanup previous tc rules for a clean slate ---
echo "--- Cleaning up existing tc rules on $INTERFACE ---"
tc qdisc del dev $INTERFACE root || true

# Function to apply shaping (htb for bandwidth + netem for latency) to a specific class
# This function will now MODIFY an existing class and/or add a netem qdisc to it.
apply_shaping_to_class() {
  local dev=$1
  local class_id=$2      # e.g., 1:20 (this is the class we are applying shaping to)

  # If throughput is set, calculate rate in kbit/s (assuming THROUGHPUT in KB/s)
  if [ ! -z "${THROUGHPUT}" ]; then
    echo "Gating network throughput for class ${class_id} at ${THROUGHPUT} KB/s."
    RATE=$((THROUGHPUT * 8))
    # MODIFIED: Use 'tc class change' to modify the rate of the existing class
    tc class change dev $dev classid ${class_id} htb rate ${RATE}kbit ceil ${RATE}kbit || true
  else
    # ORIGINAL LOGIC: The `tc class add` here was problematic as the class already exists.
    # If THROUGHPUT is NOT set, we ensure the class maintains a high rate
    # before potentially adding netem directly to it.
    # The class was already created with 1000mbit, so no change is needed here.
    echo "Throughput not set, class ${class_id} retaining default high rate."
  fi

  # If latency is set, add netem (delay in ms)
  if [ ! -z "${LATENCY}" ]; then
    echo "Gating network latency for class ${class_id} at ${LATENCY} milliseconds."
    # MODIFIED: Parent for netem is directly the class_id itself
    tc qdisc add dev $dev parent ${class_id} netem delay ${LATENCY}ms || true
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
# This class must be CREATED here before the apply_shaping_to_class function modifies it.
tc class add dev $INTERFACE parent 1: classid 1:20 htb rate 1000mbit ceil 1000mbit || true

# 4. Apply the common shaping parameters (THROUGHPUT, LATENCY) to the throttled class (1:20)
# MODIFIED: Pass only the class ID as the second argument
apply_shaping_to_class $INTERFACE "1:20"

# 5. Add filters to direct P2P traffic to the throttled class (1:20)
# CRITICAL FIX: Match BOTH source and destination ports to throttle bidirectional P2P traffic

# Filter for TCP traffic TO the P2P port (incoming connections)
tc filter add dev $INTERFACE protocol ip parent 1: prio 1 u32 \
  match ip dport $TARGET_PORT_TCP 0xffff \
  match ip protocol 6 0xff \
  flowid 1:20 || true
echo "Throttling TCP traffic TO port ${TARGET_PORT_TCP}"

# Filter for TCP traffic FROM the P2P port (outgoing data on established connections)
tc filter add dev $INTERFACE protocol ip parent 1: prio 2 u32 \
  match ip sport $TARGET_PORT_TCP 0xffff \
  match ip protocol 6 0xff \
  flowid 1:20 || true
echo "Throttling TCP traffic FROM port ${TARGET_PORT_TCP}"

# Filter for UDP traffic TO the P2P port (incoming packets)
tc filter add dev $INTERFACE protocol ip parent 1: prio 3 u32 \
  match ip dport $TARGET_PORT_UDP 0xffff \
  match ip protocol 17 0xff \
  flowid 1:20 || true
echo "Throttling UDP traffic TO port ${TARGET_PORT_UDP}"

# Filter for UDP traffic FROM the P2P port (outgoing packets)
tc filter add dev $INTERFACE protocol ip parent 1: prio 4 u32 \
  match ip sport $TARGET_PORT_UDP 0xffff \
  match ip protocol 17 0xff \
  flowid 1:20 || true
echo "Throttling UDP traffic FROM port ${TARGET_PORT_UDP}"

# ***************************** throttling connection end *****************************

# Call broadcast_network_stress_test_node
export ID=$(hostname | grep -o '[0-9]*$')
echo "Starting node with ID: $ID"
exec broadcast_network_stress_test_node