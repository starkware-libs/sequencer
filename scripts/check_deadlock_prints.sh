#!/bin/bash

# Fast path: prefer the single-pass Python implementation when available.
if command -v python3 >/dev/null 2>&1; then
    script_dir="$(cd "$(dirname "$0")" && pwd)"
    exec python3 "$script_dir/check_deadlock_prints.py" "$@"
fi

# Script to check for deadlock symptoms by comparing lock/flow markers per node.
# Analyzes commit_index, versioned state lock, RPC block_on, and client.request.

LOG_FILE="${1:-/home/arni/workspace/sequencer/sequencer_integration_test_restart.log}"

if [[ ! -f "$LOG_FILE" ]]; then
    echo "Error: Log file not found: $LOG_FILE"
    exit 1
fi

NODES=(0 1 2)

count_tag() {
    local tag="$1"
    local count
    count=$(grep -c "$tag" "$LOG_FILE" 2>/dev/null || echo "0")
    echo "$count" | tr -d '[:space:]'
}

node_prefix() {
    local template="$1"
    local node="$2"
    if [[ -z "$template" ]]; then
        template="Node {node} "
    fi
    echo "${template//\{node\}/$node}"
}

count_tag_in_node() {
    local node="$1"
    local tag="$2"
    local prefix_template="$3"
    local prefix
    local count
    prefix=$(node_prefix "$prefix_template" "$node")
    count=$(grep "$prefix" "$LOG_FILE" | grep -c "$tag" 2>/dev/null || echo "0")
    echo "$count" | tr -d '[:space:]'
}

last_line_in_node() {
    local node="$1"
    local tag="$2"
    local prefix_template="$3"
    local prefix
    prefix=$(node_prefix "$prefix_template" "$node")
    grep "$prefix" "$LOG_FILE" | grep "$tag" | tail -n 1
}

print_node_pair_section() {
    local title="$1"
    local try_tag="$2"
    local done_tag="$3"
    local last_tag="$4"
    local prefix_template="$5"
    local count_try count_done diff status last_try

    if [[ -z "$last_tag" ]]; then
        last_tag="$try_tag"
    fi

    echo "=== $title ==="
    echo ""

    for node in "${NODES[@]}"; do
        count_try=$(count_tag_in_node "$node" "$try_tag" "$prefix_template")
        count_done=$(count_tag_in_node "$node" "$done_tag" "$prefix_template")
        diff=$((count_try - count_done))

        if [[ $diff -gt 0 ]]; then
            status="STUCK"
        elif [[ $diff -eq 0 ]]; then
            status="OK"
        else
            status="UNEXPECTED"
        fi

        printf "Node %d: try=%d, done=%d, diff=%d => %s\n" "$node" "$count_try" "$count_done" "$diff" "$status"
        if [[ $diff -gt 0 ]]; then
            last_try=$(last_line_in_node "$node" "$last_tag" "$prefix_template")
            if [[ -n $last_try ]]; then
                echo "  Last try: $last_try"
            fi
        fi
    done
}

print_node_single_group() {
    local title="$1"
    local prefix_template="$2"
    local events_str="$3"
    local events

    echo "=== $title ==="
    echo ""

    IFS=';' read -r -a events <<< "$events_str"

    for node in "${NODES[@]}"; do
        for event in "${events[@]}"; do
            local tag desc count last_line
            tag="${event%%=*}"
            desc="${event#*=}"
            count=$(count_tag_in_node "$node" "$tag" "$prefix_template")
            printf "Node %d: %s (%s): %d\n" "$node" "$tag" "$desc" "$count"
            if [[ $count -gt 0 ]]; then
                last_line=$(last_line_in_node "$node" "$tag" "$prefix_template")
                if [[ -n $last_line ]]; then
                    echo "  Last: $last_line"
                fi
            fi
        done
    done
}

print_global_pair() {
    local title="$1"
    local try_tag="$2"
    local done_tag="$3"
    local last_tag="$4"
    local count_try count_done diff status last_try

    if [[ -z "$last_tag" ]]; then
        last_tag="$try_tag"
    fi

    echo "=== $title ==="
    echo ""

    count_try=$(count_tag "$try_tag")
    count_done=$(count_tag "$done_tag")
    diff=$((count_try - count_done))

    if [[ $diff -gt 0 ]]; then
        status="STUCK"
    elif [[ $diff -eq 0 ]]; then
        status="OK"
    else
        status="UNEXPECTED"
    fi

    printf "Total: try=%d, done=%d, diff=%d => %s\n" "$count_try" "$count_done" "$diff" "$status"
    if [[ $diff -gt 0 ]]; then
        last_try=$(grep "$last_tag" "$LOG_FILE" | tail -n 1)
        if [[ -n $last_try ]]; then
            echo "  Last try: $last_try"
        fi
    fi
}

print_global_single_group() {
    local title="$1"
    local events_str="$2"
    local events

    echo "=== $title ==="
    echo ""

    IFS=';' read -r -a events <<< "$events_str"
    for event in "${events[@]}"; do
        local tag desc count last_line
        tag="${event%%=*}"
        desc="${event#*=}"
        count=$(count_tag "$tag")
        printf "%s (%s): %d\n" "$tag" "$desc" "$count"
        if [[ $count -gt 0 ]]; then
            last_line=$(grep "$tag" "$LOG_FILE" | tail -n 1)
            if [[ -n $last_line ]]; then
                echo "  Last: $last_line"
            fi
        fi
    done
}

NODE_PAIR_SECTIONS=(
    "TEMPDEBUG100 (trying commit_index lock for get_n_committed_txs) vs TEMPDEBUG101 (acquired)|TEMPDEBUG100|TEMPDEBUG101|TEMPDEBUG100 |Node {node} distributed_batcher:"
    "TEMPDEBUG102 (trying commit_index lock for commit phase) vs TEMPDEBUG103 (acquired)|TEMPDEBUG102|TEMPDEBUG103|TEMPDEBUG102 |Node {node} distributed_batcher:"
    "TEMPDEBUG200 (trying versioned state lock) vs TEMPDEBUG201 (acquired)|TEMPDEBUG200 |TEMPDEBUG201 |TEMPDEBUG200 |Node {node} distributed_batcher:"
    "TEMPDEBUG300 (trying block_on get_executable) vs TEMPDEBUG301 (done)|TEMPDEBUG300 |TEMPDEBUG301 |TEMPDEBUG300 |Node {node} distributed_batcher:"
    "TEMPDEBUG400 (trying client.request) vs TEMPDEBUG401 (done)|TEMPDEBUG400 |TEMPDEBUG401 |TEMPDEBUG400 |Node {node} "
    "TEMPDEBUG500 (calling serve_connection) vs TEMPDEBUG501 (returned)|TEMPDEBUG500 |TEMPDEBUG501 |TEMPDEBUG500 |Node {node} "
    "TEMPDEBUG550 (server received request) vs TEMPDEBUG551 (server sending response)|TEMPDEBUG550 |TEMPDEBUG551 |TEMPDEBUG550 |Node {node} "
)

NODE_SINGLE_GROUPS=(
    "TEMPDEBUG552 (server response body frame)|Node {node} |TEMPDEBUG552=server response body frame"
)

GLOBAL_PAIR_SECTIONS=()
GLOBAL_SINGLE_GROUPS=()

print_pending_requests_400() {
    echo "=== TEMPDEBUG400 pending by request_id (requires request_id in log) ==="
    echo ""
    awk '
    /TEMPDEBUG40[01]/ {
        node="?";
        if (match($0, /Node ([0-9]+)/, m)) node=m[1];
        rid=""; req="";
        if (match($0, /request_id=([^ ]+)/, m)) rid=m[1];
        if (match($0, /request=([^ ]+)/, m)) req=m[1];
        if (rid == "") next;
        key = node "|" rid;
        if ($0 ~ /TEMPDEBUG400/) {
            pending[key]++;
            reqs[key]=req;
            last[key]=$0;
        } else if ($0 ~ /TEMPDEBUG401/) {
            if (pending[key] > 0) pending[key]--;
        }
    }
    END {
        for (k in pending) {
            if (pending[k] > 0) {
                split(k, parts, "|");
                node=parts[1]; rid=parts[2];
                printf("Node %s: request_id=%s request=%s pending=%d\n", node, rid, reqs[k], pending[k]);
                if (last[k] != "") printf("  Last: %s\n", last[k]);
            }
        }
    }' "$LOG_FILE"
    echo ""
}

echo "Analyzing: $LOG_FILE"

echo ""
for entry in "${NODE_PAIR_SECTIONS[@]}"; do
    IFS='|' read -r title try_tag done_tag last_tag prefix_template <<< "$entry"
    print_node_pair_section "$title" "$try_tag" "$done_tag" "$last_tag" "$prefix_template"
    echo ""
done

for group in "${NODE_SINGLE_GROUPS[@]}"; do
    IFS='|' read -r title prefix_template events_str <<< "$group"
    print_node_single_group "$title" "$prefix_template" "$events_str"
    echo ""
done

for entry in "${GLOBAL_PAIR_SECTIONS[@]}"; do
    IFS='|' read -r title try_tag done_tag last_tag _ <<< "$entry"
    print_global_pair "$title" "$try_tag" "$done_tag" "$last_tag"
    echo ""
done

for group in "${GLOBAL_SINGLE_GROUPS[@]}"; do
    IFS='|' read -r title events_str <<< "$group"
    print_global_single_group "$title" "$events_str"
    echo ""
done

print_pending_requests_400

echo "=== Summary ==="
for entry in "${NODE_PAIR_SECTIONS[@]}" "${GLOBAL_PAIR_SECTIONS[@]}"; do
    IFS='|' read -r _ try_tag done_tag _ _ <<< "$entry"
    total_try=$(count_tag "$try_tag")
    total_done=$(count_tag "$done_tag")
    diff=$((total_try - total_done))
    echo "Total ${try_tag}/${done_tag}: $total_try / $total_done (diff: $diff)"
done

for group in "${NODE_SINGLE_GROUPS[@]}" "${GLOBAL_SINGLE_GROUPS[@]}"; do
    IFS='|' read -r _ _ events_str <<< "$group"
    IFS=';' read -r -a events <<< "$events_str"
    for event in "${events[@]}"; do
        tag="${event%%=*}"
        total=$(count_tag "$tag")
        echo "Total $tag: $total"
    done
done
