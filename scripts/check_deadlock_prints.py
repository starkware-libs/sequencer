#!/usr/bin/env python3
import os
import re
import sys
from collections import Counter, defaultdict


DEFAULT_LOG = "/home/arni/workspace/sequencer/sequencer_integration_test_restart.log"
NODES = ("0", "1", "2")

NODE_PAIR_SECTIONS = [
    {
        "title": "TEMPDEBUG100 (trying commit_index lock for get_n_committed_txs) vs TEMPDEBUG101 (acquired)",
        "try_tag": "100",
        "done_tag": "101",
        "last_try": "100",
        "prefix_template": "Node {node} distributed_batcher:",
    },
    {
        "title": "TEMPDEBUG102 (trying commit_index lock for commit phase) vs TEMPDEBUG103 (acquired)",
        "try_tag": "102",
        "done_tag": "103",
        "last_try": "102",
        "prefix_template": "Node {node} distributed_batcher:",
    },
    {
        "title": "TEMPDEBUG200 (trying versioned state lock) vs TEMPDEBUG201 (acquired)",
        "try_tag": "200",
        "done_tag": "201",
        "last_try": "200",
        "prefix_template": "Node {node} distributed_batcher:",
    },
    {
        "title": "TEMPDEBUG300 (trying block_on get_executable) vs TEMPDEBUG301 (done)",
        "try_tag": "300",
        "done_tag": "301",
        "last_try": "300",
        "prefix_template": "Node {node} distributed_batcher:",
    },
    {
        "title": "TEMPDEBUG400 (trying client.request) vs TEMPDEBUG401 (done)",
        "try_tag": "400",
        "done_tag": "401",
        "last_try": "400",
        "prefix_template": "Node {node} ",
    },
    {
        "title": "TEMPDEBUG500 (calling serve_connection) vs TEMPDEBUG501 (returned)",
        "try_tag": "500",
        "done_tag": "501",
        "last_try": "500",
        "prefix_template": "Node {node} ",
    },
    {
        "title": "TEMPDEBUG550 (server received request) vs TEMPDEBUG551 (server sending response)",
        "try_tag": "550",
        "done_tag": "551",
        "last_try": "550",
        "prefix_template": "Node {node} ",
    },
]

NODE_SINGLE_GROUPS = [
    {
        "title": "TEMPDEBUG552 (server response body frame)",
        "prefix_template": "Node {node} ",
        "events": [
            ("552", "server response body frame"),
        ],
    },
]

GLOBAL_PAIR_SECTIONS = []
GLOBAL_SINGLE_GROUPS = []

RE_NODE = re.compile(r"Node\s+(\d+)")
RE_TEMP = re.compile(r"TEMPDEBUG(\d+)")
RE_REQ_ID = re.compile(r"request_id=([^ ]+)")
RE_REQ_NAME = re.compile(r"request=([^ ]+)")


def node_prefix(template, node):
    if not template:
        return f"Node {node} "
    return template.format(node=node)


def print_node_pair_section(section, counts, last_try):
    print(f"=== {section['title']} ===")
    print("")
    for node in NODES:
        try_count = counts[node]["try"]
        done_count = counts[node]["done"]
        diff = try_count - done_count
        if diff > 0:
            status = "STUCK"
        elif diff == 0:
            status = "OK"
        else:
            status = "UNEXPECTED"
        print(f"Node {node}: try={try_count}, done={done_count}, diff={diff} => {status}")
        if diff > 0:
            last_line = last_try.get(node)
            if last_line:
                print(f"  Last try: {last_line}")


def print_node_single_group(group, counts, last_lines):
    print(f"=== {group['title']} ===")
    print("")
    for node in NODES:
        for tag, desc in group["events"]:
            count = counts[node].get(tag, 0)
            print(f"Node {node}: TEMPDEBUG{tag} ({desc}): {count}")
            if count > 0:
                last_line = last_lines[node].get(tag)
                if last_line:
                    print(f"  Last: {last_line}")


def print_global_pair_section(section, counts, last_try):
    print(f"=== {section['title']} ===")
    print("")
    diff = counts["try"] - counts["done"]
    if diff > 0:
        status = "STUCK"
    elif diff == 0:
        status = "OK"
    else:
        status = "UNEXPECTED"
    print(f"Total: try={counts['try']}, done={counts['done']}, diff={diff} => {status}")
    if diff > 0 and last_try:
        print(f"  Last try: {last_try}")


def print_global_single_group(group, counts, last_lines):
    print(f"=== {group['title']} ===")
    print("")
    for tag, desc in group["events"]:
        count = counts.get(tag, 0)
        print(f"TEMPDEBUG{tag} ({desc}): {count}")
        if count > 0:
            last_line = last_lines.get(tag)
            if last_line:
                print(f"  Last: {last_line}")


def print_pending_requests_400(pending, last_line, req_name):
    print("=== TEMPDEBUG400 pending by request_id (requires request_id in log) ===")
    print("")
    for (node, req_id) in sorted(pending.keys()):
        count = pending[(node, req_id)]
        if count <= 0:
            continue
        name = req_name.get((node, req_id), "?")
        print(f"Node {node}: request_id={req_id} request={name} pending={count}")
        line = last_line.get((node, req_id))
        if line:
            print(f"  Last: {line}")
    print("")


def main():
    log_file = sys.argv[1] if len(sys.argv) > 1 else DEFAULT_LOG
    if not os.path.isfile(log_file):
        print(f"Error: Log file not found: {log_file}")
        return 1

    # Precompute prefixes
    pair_prefixes = []
    for section in NODE_PAIR_SECTIONS:
        per_node = {node: node_prefix(section.get("prefix_template"), node) for node in NODES}
        pair_prefixes.append(per_node)

    single_prefixes = []
    for group in NODE_SINGLE_GROUPS:
        per_node = {node: node_prefix(group.get("prefix_template"), node) for node in NODES}
        single_prefixes.append(per_node)

    # Maps from tag to sections/groups
    tag_to_pair_try = defaultdict(list)
    tag_to_pair_done = defaultdict(list)
    for idx, section in enumerate(NODE_PAIR_SECTIONS):
        tag_to_pair_try[section["try_tag"]].append(idx)
        tag_to_pair_done[section["done_tag"]].append(idx)

    tag_to_single_groups = defaultdict(list)
    for idx, group in enumerate(NODE_SINGLE_GROUPS):
        for tag, _ in group["events"]:
            tag_to_single_groups[tag].append(idx)

    # Counts
    pair_counts = []
    pair_last_try = []
    for _ in NODE_PAIR_SECTIONS:
        pair_counts.append({node: {"try": 0, "done": 0} for node in NODES})
        pair_last_try.append({node: None for node in NODES})

    single_counts = []
    single_last = []
    for group in NODE_SINGLE_GROUPS:
        single_counts.append({node: Counter() for node in NODES})
        single_last.append({node: {} for node in NODES})

    global_pair_counts = []
    global_pair_last = []
    for section in GLOBAL_PAIR_SECTIONS:
        global_pair_counts.append({"try": 0, "done": 0})
        global_pair_last.append(None)

    global_single_counts = []
    global_single_last = []
    for group in GLOBAL_SINGLE_GROUPS:
        global_single_counts.append(Counter())
        global_single_last.append({})

    tag_totals = Counter()

    # Pending request_id tracking for TEMPDEBUG400/401
    pending = defaultdict(int)
    pending_last = {}
    pending_req = {}

    with open(log_file, "r", encoding="utf-8", errors="replace") as handle:
        for raw in handle:
            line = raw.rstrip("\n")
            temp_match = RE_TEMP.search(line)
            if not temp_match:
                continue
            tag = temp_match.group(1)
            tag_totals[tag] += 1

            node_match = RE_NODE.search(line)
            node = node_match.group(1) if node_match else None

            if node in NODES:
                if tag in tag_to_pair_try:
                    for idx in tag_to_pair_try[tag]:
                        if pair_prefixes[idx][node] in line:
                            pair_counts[idx][node]["try"] += 1
                            pair_last_try[idx][node] = line
                if tag in tag_to_pair_done:
                    for idx in tag_to_pair_done[tag]:
                        if pair_prefixes[idx][node] in line:
                            pair_counts[idx][node]["done"] += 1

                if tag in tag_to_single_groups:
                    for idx in tag_to_single_groups[tag]:
                        if single_prefixes[idx][node] in line:
                            single_counts[idx][node][tag] += 1
                            single_last[idx][node][tag] = line

            # Global sections (if any)
            if tag in tag_to_pair_try and GLOBAL_PAIR_SECTIONS:
                for idx, section in enumerate(GLOBAL_PAIR_SECTIONS):
                    if section["try_tag"] == tag:
                        global_pair_counts[idx]["try"] += 1
                        global_pair_last[idx] = line
                    if section["done_tag"] == tag:
                        global_pair_counts[idx]["done"] += 1
            if GLOBAL_SINGLE_GROUPS:
                for idx, group in enumerate(GLOBAL_SINGLE_GROUPS):
                    for gtag, _ in group["events"]:
                        if gtag == tag:
                            global_single_counts[idx][tag] += 1
                            global_single_last[idx][tag] = line

            # Pending TEMPDEBUG400/401 with request_id
            if tag in ("400", "401"):
                req_id_match = RE_REQ_ID.search(line)
                if req_id_match:
                    req_id = req_id_match.group(1)
                    req_match = RE_REQ_NAME.search(line)
                    req_name = req_match.group(1) if req_match else "?"
                    key = (node or "?", req_id)
                    if tag == "400":
                        pending[key] += 1
                        pending_last[key] = line
                        pending_req[key] = req_name
                    else:
                        if pending[key] > 0:
                            pending[key] -= 1

    print(f"Analyzing: {log_file}")
    print("")

    for idx, section in enumerate(NODE_PAIR_SECTIONS):
        print_node_pair_section(section, pair_counts[idx], pair_last_try[idx])
        print("")

    for idx, group in enumerate(NODE_SINGLE_GROUPS):
        print_node_single_group(group, single_counts[idx], single_last[idx])
        print("")

    for idx, section in enumerate(GLOBAL_PAIR_SECTIONS):
        print_global_pair_section(section, global_pair_counts[idx], global_pair_last[idx])
        print("")

    for idx, group in enumerate(GLOBAL_SINGLE_GROUPS):
        print_global_single_group(group, global_single_counts[idx], global_single_last[idx])
        print("")

    print_pending_requests_400(pending, pending_last, pending_req)

    print("=== Summary ===")
    for section in NODE_PAIR_SECTIONS + GLOBAL_PAIR_SECTIONS:
        try_tag = section["try_tag"]
        done_tag = section["done_tag"]
        total_try = tag_totals.get(try_tag, 0)
        total_done = tag_totals.get(done_tag, 0)
        diff = total_try - total_done
        print(f"Total TEMPDEBUG{try_tag}/TEMPDEBUG{done_tag}: {total_try} / {total_done} (diff: {diff})")

    for group in NODE_SINGLE_GROUPS + GLOBAL_SINGLE_GROUPS:
        for tag, _ in group["events"]:
            total = tag_totals.get(tag, 0)
            print(f"Total TEMPDEBUG{tag}: {total}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
