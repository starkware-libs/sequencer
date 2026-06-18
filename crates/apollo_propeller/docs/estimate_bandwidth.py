"""Propeller stake-weighted sharding bandwidth estimation.

Five functions compute the estimated upload per peer for each of the five
sharding options described in propeller_stake_weighted_sharding.md.

Options 1-3: publisher has NO shard (excluded or node-counting).
Options 4-5: publisher has OWN shard(s), broadcasts them directly.

Stakes are provided as a list of positive integers sorted descending. The units
are arbitrary (basis points, absolute tokens, etc.) — only relative proportions
matter. The proposer is identified by index into that list.

Usage:
    stakes = [3200, 500] + [100] * 63  # basis points, sorted descending
    uploads = option5_uploads(stakes, message_size=5.0, proposer=0, T=65)
"""

from __future__ import annotations


# ---------------------------------------------------------------------------
# Option 1: Node Counting (current implementation)
# ---------------------------------------------------------------------------

def option1_uploads(
    stakes: list[int],
    message_size: float,
    proposer: int,
) -> list[float]:
    """Estimate per-peer upload for Option 1 (node counting).

    Every receiver gets exactly one shard.  Thresholds are based on shard
    count, not stake.  All receivers upload the same amount.

    Args:
        stakes:       Stake amounts per node, sorted descending.
        message_size: Message size in MiB (or MiB/s for throughput).
        proposer:     Index of the proposer in *stakes*.

    Returns:
        List of estimated uploads (same units as *message_size*), one per node.
    """
    _validate_inputs(stakes, proposer)

    n = len(stakes)
    d = max(1, (n - 1) // 3)  # num_data_shards
    shard_size = message_size / d

    uploads: list[float] = []
    for i in range(n):
        if i == proposer:
            # Publisher sends one shard to each of N-1 receivers.
            uploads.append((n - 1) * shard_size)
        else:
            # Receiver gossips their shard to N-2 other peers.
            uploads.append((n - 2) * shard_size)
    return uploads


# ---------------------------------------------------------------------------
# Option 2: Stake Counting, Publisher Excluded, Fixed Allocation
# ---------------------------------------------------------------------------

def option2_uploads(
    stakes: list[int],
    message_size: float,
    proposer: int,
) -> list[float]:
    """Estimate per-peer upload for Option 2 (publisher excluded, fixed alloc).

    Every receiver still gets exactly one shard, but the build threshold is based
    on *non-publisher stake* S' = S - s_p, and the receive threshold is
    2S/3 - s_p.  K' = smallest number of top-stake validators whose combined
    stake reaches S'/3.

    Args:
        stakes:       Stake amounts per node, sorted descending.
        message_size: Message size in MiB (or MiB/s for throughput).
        proposer:     Index of the proposer in *stakes*.

    Returns:
        List of estimated uploads, one per node.
    """
    _validate_inputs(stakes, proposer)

    n = len(stakes)
    total_stake = sum(stakes)
    s_p = stakes[proposer]
    s_prime = total_stake - s_p  # non-publisher stake

    # Validators = everyone except the proposer, sorted descending.
    validator_stakes = sorted(
        (stakes[i] for i in range(n) if i != proposer),
        reverse=True,
    )

    # K' = smallest k s.t.  s_1 + ... + s_k  >=  S' / 3
    # Using integer arithmetic: 3 * cumulative >= s_prime.
    cumulative = 0
    k_prime = 0
    for vs in validator_stakes:
        cumulative += vs
        k_prime += 1
        if 3 * cumulative >= s_prime:
            break
    k_prime = max(k_prime, 1)

    shard_size = message_size / k_prime

    uploads: list[float] = []
    for i in range(n):
        if i == proposer:
            uploads.append((n - 1) * shard_size)
        else:
            uploads.append((n - 2) * shard_size)
    return uploads


# ---------------------------------------------------------------------------
# Option 3: Stake Counting, Publisher Excluded, Proportional Allocation
# ---------------------------------------------------------------------------

def option3_uploads(
    stakes: list[int],
    message_size: float,
    proposer: int,
    T: int,  # noqa: N803 – matches the document's notation
) -> list[float]:
    """Estimate per-peer upload for Option 3 (publisher excluded, proportional).

    Shards are allocated via a greedy algorithm among the N-1 receivers.
    Starting with 1 shard per receiver, the algorithm repeatedly gives an
    additional shard to the receiver with the highest stake/num_shards ratio,
    until T shards have been assigned.

    The build threshold is S'/3 (non-publisher stake); the receive threshold
    is 2S/3 - s_p.  num_data_shards is the minimum number of shards held by
    any receiver coalition whose combined stake reaches S'/3.

    Args:
        stakes:       Stake amounts per node, sorted descending.
        message_size: Message size in MiB (or MiB/s for throughput).
        proposer:     Index of the proposer in *stakes*.
        T:            Total number of shards (must be >= N-1).

    Returns:
        List of estimated uploads, one per node.
    """
    _validate_inputs(stakes, proposer)

    n = len(stakes)
    assert T >= n - 1, f"T must be >= N-1 (got T={T}, N-1={n - 1})"

    total_stake = sum(stakes)
    s_p = stakes[proposer]
    s_prime = total_stake - s_p  # non-publisher stake

    # Build receiver list: (original_index, stake) for non-publisher nodes.
    receivers = [(i, stakes[i]) for i in range(n) if i != proposer]

    # Greedy shard allocation among receivers.
    shard_counts = _greedy_shard_allocation(
        node_stakes=[s for _, s in receivers],
        total_shards=T,
    )
    # Map back: shard_alloc[original_index] = num_shards for that node.
    shard_alloc: dict[int, int] = {}
    for idx, (orig_i, _) in enumerate(receivers):
        shard_alloc[orig_i] = shard_counts[idx]

    # num_data_shards: minimum number of shards held by any receiver coalition
    # whose combined stake reaches S'/3.
    num_data = _min_shards_for_stake_threshold(
        node_stakes=[s for _, s in receivers],
        node_shards=shard_counts,
        threshold=s_prime,  # integer threshold: 3*cumulative >= s_prime
    )

    shard_size = message_size / num_data

    # Publisher uploads all T shards (one to each receiver's designated slot).
    publisher_upload = T * shard_size

    uploads: list[float] = []
    for i in range(n):
        if i == proposer:
            uploads.append(publisher_upload)
        else:
            # Receiver gossips their shard(s) to N-2 peers.
            upload = shard_alloc[i] * (n - 2) * shard_size
            uploads.append(upload)
    return uploads


# ---------------------------------------------------------------------------
# Option 4: Stake Counting, Publisher in Pool, Fixed Allocation
# ---------------------------------------------------------------------------

def option4_uploads(
    stakes: list[int],
    message_size: float,
    proposer: int,
) -> list[float]:
    """Estimate per-peer upload for Option 4 (publisher in pool, fixed alloc).

    Every node (including the publisher) gets exactly one shard.  N total shards.
    Thresholds are based on *stake* over all N nodes (no "free" publisher stake).
    K = smallest number of top-stake nodes whose combined stake reaches S/3.
    K is publisher-independent.

    Publisher distributes N-1 shards + broadcasts own shard to N-1 peers.
    Publisher upload = 2(N-1) × shard_size.
    Receiver upload = (N-2) × shard_size.

    Args:
        stakes:       Stake amounts per node, sorted descending.
        message_size: Message size in MiB (or MiB/s for throughput).
        proposer:     Index of the proposer in *stakes*.

    Returns:
        List of estimated uploads, one per node.
    """
    _validate_inputs(stakes, proposer)

    n = len(stakes)
    total_stake = sum(stakes)

    # K = smallest k s.t. top-k of ALL nodes' stakes >= total_stake / 3.
    # Publisher-independent: uses all N nodes sorted descending.
    all_sorted = sorted(stakes, reverse=True)
    cumulative = 0
    k = 0
    for s in all_sorted:
        cumulative += s
        k += 1
        if 3 * cumulative >= total_stake:
            break
    k = max(k, 1)

    shard_size = message_size / k

    uploads: list[float] = []
    for i in range(n):
        if i == proposer:
            # Publisher: distribute N-1 shards + broadcast own shard to N-1 peers.
            uploads.append(2 * (n - 1) * shard_size)
        else:
            # Receiver: gossip own shard to N-2 peers.
            uploads.append((n - 2) * shard_size)
    return uploads


# ---------------------------------------------------------------------------
# Option 5: Stake Counting, Publisher in Pool, Proportional Allocation
# ---------------------------------------------------------------------------

def option5_uploads(
    stakes: list[int],
    message_size: float,
    proposer: int,
    T: int,  # noqa: N803 – matches the document's notation
) -> list[float]:
    """Estimate per-peer upload for Option 5 (publisher in pool, proportional).

    Shards are allocated via a greedy algorithm among all N nodes (including
    the publisher).  Starting with 1 shard per node, the algorithm repeatedly
    gives an additional shard to the node with the highest stake/num_shards
    ratio, until T shards have been assigned.

    The build threshold is S/3 (total stake); the receive threshold is 2S/3.
    num_data_shards is the minimum number of shards held by any coalition
    whose combined stake reaches S/3.

    The publisher broadcasts its own shards to all N-1 receivers and sends
    each receiver's shards to that receiver.

    Args:
        stakes:       Stake amounts per node, sorted descending.
        message_size: Message size in MiB (or MiB/s for throughput).
        proposer:     Index of the proposer in *stakes*.
        T:            Total number of shards (must be >= N).

    Returns:
        List of estimated uploads, one per node.
    """
    _validate_inputs(stakes, proposer)

    n = len(stakes)
    assert T >= n, f"T must be >= N (got T={T}, N={n})"

    total_stake = sum(stakes)

    # Greedy shard allocation among all N nodes.
    shard_counts = _greedy_shard_allocation(
        node_stakes=list(stakes),
        total_shards=T,
    )

    # num_data_shards: minimum shards in any coalition with stake >= S/3.
    num_data = _min_shards_for_stake_threshold(
        node_stakes=list(stakes),
        node_shards=shard_counts,
        threshold=total_stake,
    )

    shard_size = message_size / num_data

    pub_shards = shard_counts[proposer]
    non_pub_shards = T - pub_shards

    # Publisher sends each receiver's shards to that receiver (non_pub_shards
    # shard transmissions) and broadcasts its own shards to all N-1 receivers
    # (pub_shards * (N-1) transmissions).
    publisher_upload = (non_pub_shards + pub_shards * (n - 1)) * shard_size

    uploads: list[float] = []
    for i in range(n):
        if i == proposer:
            uploads.append(publisher_upload)
        else:
            # Receiver gossips their shard(s) to N-2 peers.
            upload = shard_counts[i] * (n - 2) * shard_size
            uploads.append(upload)
    return uploads


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def compute_s_coal(stakes: list[int]) -> int:
    """Minimum subset sum of *stakes* that is >= ceil(total / 3).

    Uses a Python big-int as a bitset of achievable subset sums.  Bit *i* is
    set iff some subset of *stakes* sums to exactly *i*.  After building the
    bitset, we find the lowest set bit at or above the threshold.

    Complexity: O(N * total / word_size) — fast in practice because Python's
    arbitrary-precision integer operations are implemented in C.
    """
    total = sum(stakes)
    # Integer threshold: need subset sum S with 3*S >= total.
    threshold = -(-total // 3)  # ceil(total / 3)

    achievable = 1  # bit 0 set → empty subset has sum 0
    for s in stakes:
        achievable |= achievable << s

    # Shift so that bit 0 corresponds to sum = threshold.
    mask = achievable >> threshold
    if mask == 0:
        return total  # fallback (shouldn't happen if threshold <= total)

    lowest_bit_pos = (mask & -mask).bit_length() - 1
    return threshold + lowest_bit_pos


def _compute_s_coal_subset(stakes: list[int], total: int) -> int:
    """Minimum subset sum of *stakes* that is >= ceil(total / 3).

    Same algorithm as compute_s_coal but takes explicit total (for when the
    threshold is computed against a subset total, e.g. non-publisher stake).
    """
    threshold = -(-total // 3)  # ceil(total / 3)

    achievable = 1
    for s in stakes:
        achievable |= achievable << s

    mask = achievable >> threshold
    if mask == 0:
        return total

    lowest_bit_pos = (mask & -mask).bit_length() - 1
    return threshold + lowest_bit_pos


def _greedy_shard_allocation(
    node_stakes: list[int],
    total_shards: int,
) -> list[int]:
    """Allocate shards greedily to approximate proportional-to-stake allocation.

    Starts with 1 shard per node, then repeatedly gives an additional shard to
    the node with the highest stake / num_shards ratio until *total_shards*
    have been assigned.

    This is practical because Reed-Solomon cannot handle arbitrarily large T;
    with T = 2× the base shard count, the greedy allocation is close to the
    ideal proportional allocation.

    Args:
        node_stakes:   Stake per node (positive integers).
        total_shards:  Total shards to distribute (must be >= len(node_stakes)).

    Returns:
        List of shard counts, one per node, summing to *total_shards*.
    """
    n = len(node_stakes)
    assert total_shards >= n, (
        f"total_shards ({total_shards}) must be >= number of nodes ({n})"
    )

    shards = [1] * n
    remaining = total_shards - n

    for _ in range(remaining):
        # Find the node with the highest stake / num_shards ratio.
        # Equivalent to: argmax_i( stake_i / shards_i ).
        # To avoid float division, compare stake_i * shards_j vs stake_j * shards_i.
        best = 0
        for j in range(1, n):
            if node_stakes[j] * shards[best] > node_stakes[best] * shards[j]:
                best = j
        shards[best] += 1

    return shards


def _min_shards_for_stake_threshold(
    node_stakes: list[int],
    node_shards: list[int],
    threshold: int,
) -> int:
    """Minimum number of shards held by any coalition with stake >= threshold/3.

    This is the num_data_shards for proportional-allocation options: the
    erasure coding must allow reconstruction from any set of nodes whose
    combined stake reaches the build threshold.  The binding coalition is the
    qualifying set with the *fewest shards*.

    Uses a DP approach: track achievable (stake, shards) pairs and find the
    minimum shards among those with stake >= ceil(threshold / 3).

    Args:
        node_stakes:  Stake per node.
        node_shards:  Shard count per node.
        threshold:    The total stake to measure against (3 * accumulated >= threshold).

    Returns:
        The minimum number of shards in any qualifying coalition.
    """
    target = -(-threshold // 3)  # ceil(threshold / 3)

    # dp[stake_sum] = minimum shards to achieve exactly stake_sum.
    # We only need stake sums up to sum(node_stakes).
    max_stake = sum(node_stakes)
    INF = float("inf")
    dp = [INF] * (max_stake + 1)
    dp[0] = 0

    for stake_i, shards_i in zip(node_stakes, node_shards):
        # Traverse in reverse to avoid using the same node twice.
        for s in range(max_stake, stake_i - 1, -1):
            prev = dp[s - stake_i]
            if prev < INF:
                candidate = prev + shards_i
                if candidate < dp[s]:
                    dp[s] = candidate

    best = INF
    for s in range(target, max_stake + 1):
        if dp[s] < best:
            best = dp[s]

    assert best < INF, "No qualifying coalition found (should not happen)"
    return int(best)


def _validate_inputs(stakes: list[int], proposer: int) -> None:
    assert len(stakes) >= 2, "Need at least 2 nodes"
    assert all(s > 0 for s in stakes), "All stakes must be positive"
    assert stakes == sorted(stakes, reverse=True), "Stakes must be sorted descending"
    assert 0 <= proposer < len(stakes), "Proposer index out of range"


# ---------------------------------------------------------------------------
# Stake distribution builder
# ---------------------------------------------------------------------------

def make_stakes(N: int, max_stake_bps: int, total_bps: int = 10_000) -> list[int]:
    """Build a stake pool: [max_stake, max_stake, …, max_stake, remainder].

    Fill as many validators as possible at *max_stake_bps*, then one validator
    gets whatever is left.  The list is sorted descending (as required by the
    upload functions).

    Args:
        N:              Total number of nodes (including publisher).
        max_stake_bps:  Maximum stake per validator in basis points.
        total_bps:      Total stake pool in basis points (default 10 000 = 100%).

    Returns:
        List of length N, sorted descending, summing to *total_bps*.
    """
    assert N >= 2, "Need at least 2 nodes"
    assert 0 < max_stake_bps <= total_bps, "max_stake must be in (0, total]"

    full_count = total_bps // max_stake_bps  # how many can get max_stake
    remainder = total_bps - full_count * max_stake_bps

    if full_count >= N:
        # Everyone can get max_stake.  Put N−1 nodes at max_stake, one gets
        # the remainder (total_bps − (N−1) × max_stake_bps).
        remainder_here = total_bps - (N - 1) * max_stake_bps
        if remainder_here >= max_stake_bps:
            # Even division is fine; give each total_bps // N, last gets leftover.
            per_node = total_bps // N
            leftover = total_bps - per_node * N
            stakes = [per_node] * N
            stakes[0] += leftover  # keep sorted descending
            return stakes
        stakes = [max_stake_bps] * (N - 1) + [remainder_here]
        stakes.sort(reverse=True)
        assert all(s > 0 for s in stakes)
        return stakes

    if remainder == 0:
        # Exact fit — but we need N nodes, and full_count < N.
        # Steal 1 bp from the last full-stake node and give it to the remainder.
        # Actually: full_count nodes at max_stake sum to total_bps exactly,
        # but we need N - full_count more nodes.  Redistribute: take from
        # the full nodes to create the remaining slots (each with ≥ 1 bp).
        extra_needed = N - full_count
        # Each of the extra nodes gets 1 bp; steal from the full nodes.
        # Reduce one full node by extra_needed bps.
        stakes = [max_stake_bps] * (full_count - 1) + \
                 [max_stake_bps - extra_needed] + \
                 [1] * extra_needed
        # Edge case: if max_stake_bps - extra_needed < 1, we need a different
        # split; for practical validator counts this won't happen.
        assert all(s > 0 for s in stakes), (
            f"Cannot split {total_bps} bps across {N} nodes with max {max_stake_bps}"
        )
        stakes.sort(reverse=True)
        return stakes

    # General case: full_count nodes at max_stake + 1 node with remainder.
    # If full_count + 1 < N, we need more nodes — split the remainder.
    nodes_so_far = full_count + (1 if remainder > 0 else 0)
    if nodes_so_far >= N:
        # We have enough (or too many).  Truncate to N.
        stakes = [max_stake_bps] * min(full_count, N - 1) + [remainder]
        stakes = stakes[:N]
        # Adjust if sum != total_bps
        diff = total_bps - sum(stakes)
        stakes[-1] += diff
        stakes.sort(reverse=True)
        assert all(s > 0 for s in stakes)
        return stakes

    # Need more nodes than full_count + 1.  Split the remainder across
    # (N - full_count) nodes, each getting at least 1 bp.
    extra_nodes = N - full_count
    per_extra = remainder // extra_nodes
    leftover = remainder - per_extra * extra_nodes
    if per_extra == 0:
        # Not enough remainder to give 1 bp each; steal from full nodes.
        need = extra_nodes - remainder  # how many extra bps we need
        # Reduce full nodes by 1 bp each (steal 'need' bps).
        stolen = min(need, full_count)
        stakes = [max_stake_bps - 1] * stolen + \
                 [max_stake_bps] * (full_count - stolen)
        pool = remainder + stolen
        # Distribute pool across extra_nodes
        per_extra = pool // extra_nodes
        leftover = pool - per_extra * extra_nodes
        extra = [per_extra + 1] * leftover + [per_extra] * (extra_nodes - leftover)
        stakes = stakes + extra
    else:
        stakes = [max_stake_bps] * full_count
        extra = [per_extra + 1] * leftover + [per_extra] * (extra_nodes - leftover)
        stakes = stakes + extra

    stakes.sort(reverse=True)
    assert len(stakes) == N, f"Expected {N} nodes, got {len(stakes)}"
    assert sum(stakes) == total_bps, f"Sum {sum(stakes)} != {total_bps}"
    assert all(s > 0 for s in stakes), "All stakes must be positive"
    return stakes


# ---------------------------------------------------------------------------
# Bandwidth sweep across different max_stake cutoffs
# ---------------------------------------------------------------------------

def _compute_expansion_for_option2(stakes: list[int], proposer: int) -> float:
    """Compute expansion factor for Option 2 with the given proposer."""
    n = len(stakes)
    total_stake = sum(stakes)
    s_p = stakes[proposer]
    s_prime = total_stake - s_p
    vals = sorted((stakes[i] for i in range(n) if i != proposer), reverse=True)
    cumulative, k_prime = 0, 0
    for v in vals:
        cumulative += v
        k_prime += 1
        if 3 * cumulative >= s_prime:
            break
    k_prime = max(k_prime, 1)
    return (n - 1) / k_prime


def _compute_expansion_for_option3(
    stakes: list[int], proposer: int, T: int,
) -> float:
    """Compute expansion factor for Option 3 with greedy allocation."""
    n = len(stakes)
    total_stake = sum(stakes)
    s_p = stakes[proposer]
    s_prime = total_stake - s_p
    receiver_stakes = [stakes[i] for i in range(n) if i != proposer]
    shard_counts = _greedy_shard_allocation(receiver_stakes, T)
    num_data = _min_shards_for_stake_threshold(receiver_stakes, shard_counts, s_prime)
    return T / num_data


def _compute_expansion_for_option5(
    stakes: list[int], proposer: int, T: int,
) -> float:
    """Compute expansion factor for Option 5 with greedy allocation."""
    total_stake = sum(stakes)
    shard_counts = _greedy_shard_allocation(list(stakes), T)
    num_data = _min_shards_for_stake_threshold(list(stakes), shard_counts, total_stake)
    return T / num_data


def _worst_case_across_publishers(
    stakes: list[int],
    message_size: float,
    option_fn,
    option_fn_kwargs: dict | None = None,
    expansion_fn=None,
) -> tuple[float, float, float, float, int]:
    """Run an option with every distinct publisher and return worst-case metrics.

    Nodes with identical stake produce identical results, so we deduplicate.

    Args:
        expansion_fn: Optional callable(stakes, proposer) -> float that computes
                      the expansion factor for a given publisher.  If provided,
                      the returned expansion corresponds to the worst-case
                      publisher (not an independent maximum).

    Returns:
        (worst_max_upload, worst_pub_upload, worst_max_rcv, worst_expansion,
         worst_publisher_index)

    Where "worst" means the publisher choice that maximises the single highest
    upload across *all* nodes (publisher and receivers).
    """
    n = len(stakes)
    kwargs = option_fn_kwargs or {}

    # Deduplicate: only try one publisher index per distinct stake value.
    seen_stakes: set[int] = set()
    candidate_indices: list[int] = []
    for i in range(n):
        if stakes[i] not in seen_stakes:
            seen_stakes.add(stakes[i])
            candidate_indices.append(i)

    worst_max_upload = -1.0
    worst_pub_upload = 0.0
    worst_max_rcv = 0.0
    worst_expansion = 0.0
    worst_idx = 0

    for proposer in candidate_indices:
        uploads = option_fn(stakes, message_size, proposer, **kwargs)
        max_upload = max(uploads)
        if max_upload > worst_max_upload:
            worst_max_upload = max_upload
            worst_pub_upload = uploads[proposer]
            worst_max_rcv = max(uploads[i] for i in range(n) if i != proposer)
            worst_idx = proposer
            if expansion_fn is not None:
                worst_expansion = expansion_fn(stakes, proposer)

    return (worst_max_upload, worst_pub_upload, worst_max_rcv, worst_expansion,
            worst_idx)


def run_sweep(
    N: int = 65,
    M: float = 5.0,
    max_stake_values_pct: list[float] | None = None,
    total_bps: int = 10_000,
) -> None:
    """Print bandwidth estimates as markdown tables.

    For each max_stake percentage, builds a stake pool and runs every option
    with **every distinct node as publisher**.  Reports the worst case: the
    publisher choice that produces the highest upload requirement from any
    single node.

    Args:
        N:                    Total nodes.
        M:                    Message throughput in MiB/s.
        max_stake_values_pct: List of max-stake percentages to sweep.
                              Defaults to [1%, 2%, 5%, 10%, 15%, 20%, 25%, 32%].
        total_bps:            Total stake in basis points (default 10 000).
    """
    if max_stake_values_pct is None:
        max_stake_values_pct = [1.0, 2.0, 5.0, 10.0, 15.0, 20.0, 25.0, 32.0]

    print(f"# Worst-case bandwidth sweep: N = {N}, M = {M} MiB/s")
    print()
    print("For each stake pool and option, every distinct node is tried as publisher.")
    print("The row shows the publisher choice that maximises the peak upload from any node.")

    for pct in max_stake_values_pct:
        max_bps = int(round(pct / 100.0 * total_bps))
        if max_bps < 1:
            max_bps = 1
        stakes = make_stakes(N, max_bps, total_bps)

        # Describe distribution.
        unique: dict[int, int] = {}
        for s in stakes:
            unique[s] = unique.get(s, 0) + 1
        parts = [f"{v}\u00d7{s/100:.2f}%" for s, v in sorted(unique.items(), reverse=True)]
        dist_str = " + ".join(parts)

        # Expansion functions for publisher-independent options.
        def _exp_option1(_stakes: list[int], _proposer: int) -> float:
            return (len(_stakes) - 1) / max(1, (len(_stakes) - 1) // 3)

        def _exp_option4(_stakes: list[int], _proposer: int) -> float:
            t = sum(_stakes)
            all_sorted = sorted(_stakes, reverse=True)
            cum, k = 0, 0
            for v in all_sorted:
                cum += v; k += 1
                if 3 * cum >= t:
                    break
            return len(_stakes) / max(k, 1)

        T3 = 2 * (N - 1)
        T5 = 2 * N

        def _exp_option3_greedy(_stakes: list[int], _proposer: int) -> float:
            return _compute_expansion_for_option3(_stakes, _proposer, T3)

        def _exp_option5_greedy(_stakes: list[int], _proposer: int) -> float:
            return _compute_expansion_for_option5(_stakes, _proposer, T5)

        wc1 = _worst_case_across_publishers(
            stakes, M, option1_uploads, expansion_fn=_exp_option1,
        )
        wc2 = _worst_case_across_publishers(
            stakes, M, option2_uploads,
            expansion_fn=_compute_expansion_for_option2,
        )
        wc3 = _worst_case_across_publishers(
            stakes, M, option3_uploads, {"T": T3},
            expansion_fn=_exp_option3_greedy,
        )
        wc4 = _worst_case_across_publishers(
            stakes, M, option4_uploads, expansion_fn=_exp_option4,
        )
        wc5 = _worst_case_across_publishers(
            stakes, M, option5_uploads, {"T": T5},
            expansion_fn=_exp_option5_greedy,
        )

        def _pub_label(idx: int) -> str:
            return f"{stakes[idx]/100:.2f}%"

        print(f"\n## max\_stake = {pct:.1f}%")
        print()
        print(f"Distribution: {dist_str}")
        print()
        print("| Option | Expansion | Peak (MiB/s) | Peak (Gbps) "
              "| Pub upload | Max rcv | Worst publisher |")
        print("|---|--:|--:|--:|--:|--:|---|")
        for label, wc in [
            ("Option 1 (nodes)",    wc1),
            ("Option 2 (excl fix)", wc2),
            ("Option 3 (excl prop)",wc3),
            ("Option 4 (pool fix)", wc4),
            ("Option 5 (pool prop)",wc5),
        ]:
            peak_gbps = _mib_to_gbps(wc[0])
            print(f"| {label} | {wc[3]:.2f}\u00d7 | {wc[0]:.1f} | {peak_gbps:.2f} "
                  f"| {wc[1]:.1f} | {wc[2]:.1f} | pub={_pub_label(wc[4])} |")


def _distinct_publisher_indices(stakes: list[int]) -> list[int]:
    """Return one index per distinct stake value (deduplication for sweeps)."""
    seen: set[int] = set()
    indices: list[int] = []
    for i, s in enumerate(stakes):
        if s not in seen:
            seen.add(s)
            indices.append(i)
    return indices


# ---------------------------------------------------------------------------
# Per-validator worst-case bandwidth across all publishers
# ---------------------------------------------------------------------------

def _per_validator_worst_case(
    stakes: list[int],
    message_size: float,
    option_fn,
    option_fn_kwargs: dict | None = None,
) -> list[float]:
    """For each validator, compute their worst-case upload across all publishers.

    Tries every distinct node as publisher and, for each validator, keeps the
    maximum upload that validator ever experiences (whether as publisher or
    receiver).

    Args:
        stakes:           Stake amounts per node, sorted descending.
        message_size:     Message size in MiB (or MiB/s for throughput).
        option_fn:        One of option1_uploads .. option5_uploads.
        option_fn_kwargs: Extra kwargs forwarded to *option_fn* (e.g. T=...).

    Returns:
        List of worst-case uploads, one per node (same ordering as *stakes*).
    """
    n = len(stakes)
    kwargs = option_fn_kwargs or {}

    # worst_upload[i] = max upload node i ever experiences.
    worst_upload = [0.0] * n

    # Deduplicate publishers: nodes with identical stake produce symmetric
    # results for same-stake peers, but different results for other peers.
    # We still need to try all distinct publisher stakes.
    seen_stakes: set[int] = set()
    candidate_indices: list[int] = []
    for i in range(n):
        if stakes[i] not in seen_stakes:
            seen_stakes.add(stakes[i])
            candidate_indices.append(i)

    for proposer in candidate_indices:
        uploads = option_fn(stakes, message_size, proposer, **kwargs)
        for i in range(n):
            if uploads[i] > worst_upload[i]:
                worst_upload[i] = uploads[i]

    return worst_upload


def _mib_to_gbps(mib_per_s: float) -> float:
    """Convert MiB/s to Gbps (1 MiB = 2^20 bytes = 8 × 2^20 bits)."""
    return mib_per_s * 8 * (1024 * 1024) / 1_000_000_000


def make_tiered_stakes(
    N: int,
    max_stake_pct: float,
    tier_pcts: list[float] | None = None,
    total_bps: int = 10_000,
) -> list[int]:
    """Build a stake pool: [max, max, …, max, 20%, 15%, 10%, 5%, remainder].

    First fills as many validators as possible at *max_stake_pct*.  Then places
    one validator at each tier in *tier_pcts* that is strictly below
    *max_stake_pct*.  Finally distributes the leftover stake equally among
    the remaining nodes.

    Args:
        N:              Total number of nodes.
        max_stake_pct:  Maximum stake any single validator can have (percent).
        tier_pcts:      Percentage tiers to include (default [5, 10, 15, 20]).
                        Only tiers strictly below max_stake_pct are used.
        total_bps:      Total stake in basis points (default 10 000 = 100%).

    Returns:
        List of length N, sorted descending, summing to *total_bps*.
    """
    if tier_pcts is None:
        tier_pcts = [5.0, 10.0, 15.0, 20.0]

    max_bps = int(round(max_stake_pct / 100.0 * total_bps))

    # Tiers strictly below max_stake (so they don't duplicate the max tier).
    mid_tiers_bps = sorted(
        [int(round(p / 100.0 * total_bps)) for p in tier_pcts if p < max_stake_pct],
        reverse=True,
    )

    mid_total = sum(mid_tiers_bps)
    mid_count = len(mid_tiers_bps)

    # Fill as many nodes at max_stake as we can, reserving slots for mid tiers
    # and at least 1 remainder node.
    available_for_max = N - mid_count - 1  # at least 1 node for remainder
    assert available_for_max >= 1, "Not enough nodes for max + tiers + remainder"

    max_count = min(available_for_max, (total_bps - mid_total) // max_bps)
    assert max_count >= 1, "Cannot fit even one max-stake node"

    # If max nodes + mid tiers exactly consume all stake, reduce max_count by 1
    # so the remainder nodes get a positive share.
    if max_count * max_bps + mid_total >= total_bps:
        max_count -= 1
        assert max_count >= 1, "Cannot fit max-stake node and leave positive remainder"

    used_stake = max_count * max_bps + mid_total
    remaining_stake = total_bps - used_stake
    remaining_nodes = N - max_count - mid_count

    assert remaining_stake > 0, (
        f"Max + mid tiers consume {used_stake} of {total_bps} bps, nothing left"
    )
    assert remaining_nodes >= 1

    per_small = remaining_stake // remaining_nodes
    leftover = remaining_stake - per_small * remaining_nodes

    # Distribute leftover 1 bp at a time to keep things close to equal.
    small_stakes = [per_small + 1] * leftover + [per_small] * (remaining_nodes - leftover)

    stakes = [max_bps] * max_count + mid_tiers_bps + small_stakes
    stakes.sort(reverse=True)

    assert len(stakes) == N, f"Expected {N}, got {len(stakes)}"
    assert sum(stakes) == total_bps, f"Sum {sum(stakes)} != {total_bps}"
    assert all(s > 0 for s in stakes), "All stakes must be positive"
    return stakes


def run_per_validator_sweep(
    N: int = 65,
    M: float = 5.0,
    max_stake_values_pct: list[float] | None = None,
    total_bps: int = 10_000,
) -> None:
    """Print per-validator worst-case bandwidth as markdown tables.

    For each stake distribution, builds a tiered stake pool:
      [max, max, ..., max, 20%, 15%, 10%, 5%, remainder]
    Then computes the worst-case upload every validator will ever experience
    across all possible publishers.

    This answers the question: "If I have X% stake, what bandwidth do I need
    to provision?"

    Args:
        N:                    Total nodes.
        M:                    Message throughput in MiB/s.
        max_stake_values_pct: List of max-stake percentages to sweep.
        total_bps:            Total stake in basis points (default 10 000).
    """
    if max_stake_values_pct is None:
        max_stake_values_pct = [5.0, 10.0, 15.0, 20.0, 25.0, 32.0]

    print(f"# Per-validator worst-case bandwidth: N = {N}, M = {M} MiB/s")
    print()
    print("For each validator stake tier, shows the worst upload across ALL possible publishers.")
    print("This is the bandwidth each validator must provision.")
    print("Pool: \\[max, max, ..., max, 20%, 15%, 10%, 5%, remainder\\] (tiers < max\\_stake only).")

    option_defs = [
        ("Opt 1 (nodes)",     option1_uploads, {}),
        ("Opt 2 (excl fix)",  option2_uploads, {}),
        ("Opt 3 (excl prop)", option3_uploads, {"T": 2 * (N - 1)}),
        ("Opt 4 (pool fix)",  option4_uploads, {}),
        ("Opt 5 (pool prop)", option5_uploads, {"T": 2 * N}),
    ]

    for pct in max_stake_values_pct:
        stakes = make_tiered_stakes(N, pct, total_bps=total_bps)

        # Describe distribution.
        unique: dict[int, int] = {}
        for s in stakes:
            unique[s] = unique.get(s, 0) + 1
        parts = [f"{v}\u00d7{s/100:.2f}%" for s, v in sorted(unique.items(), reverse=True)]
        dist_str = " + ".join(parts)

        # Identify distinct stake tiers (sorted descending).
        tiers = sorted(unique.keys(), reverse=True)

        # Compute per-validator worst-case for each option.
        option_results: list[tuple[str, list[float]]] = []
        for label, fn, kwargs in option_defs:
            wc = _per_validator_worst_case(stakes, M, fn, kwargs)
            option_results.append((label, wc))

        # Section header.
        print(f"\n## max\\_stake = {pct:.1f}%")
        print()
        print(f"Distribution: {dist_str}")

        # Build tier headers.
        tier_headers = [f"{s/100:.2f}% ({unique[s]})" for s in tiers]

        # --- MiB/s table ---
        print()
        print("### Upload (MiB/s)")
        print()
        header = "| Option | " + " | ".join(tier_headers) + " |"
        sep = "|---| " + " | ".join(["--:" for _ in tiers]) + " |"
        print(header)
        print(sep)
        for label, wc in option_results:
            cells: list[str] = []
            for s in tiers:
                idx = next(i for i, st in enumerate(stakes) if st == s)
                cells.append(f"{wc[idx]:.1f}")
            print(f"| {label} | " + " | ".join(cells) + " |")

        # --- Gbps table ---
        print()
        print("### Upload (Gbps)")
        print()
        print(header)
        print(sep)
        for label, wc in option_results:
            cells = []
            for s in tiers:
                idx = next(i for i, st in enumerate(stakes) if st == s)
                cells.append(f"{_mib_to_gbps(wc[idx]):.2f}")
            print(f"| {label} | " + " | ".join(cells) + " |")


# ---------------------------------------------------------------------------
# Option 3 vs Option 5 comparison (markdown table + Mermaid chart)
# ---------------------------------------------------------------------------

def print_opt3_vs_opt5(
    N: int = 65,
    M: float = 5.0,
    max_stake_values_pct: list[float] | None = None,
    total_bps: int = 10_000,
) -> None:
    """Print a markdown comparison of Option 3 vs Option 5 bandwidth.

    Tracks four fixed stake-tier series (20%, 15%, 10%, 5%) across a sweep
    of max_stake distributions.  For each distribution, computes the
    worst-case upload for every validator tier under both options, then prints:

      1. Per-distribution data tables with Opt 3, Opt 5, and ratio.
      2. A summary ratio table for the chart series.
      3. Dense Mermaid xycharts (1% max_stake steps) for each tier, paired
         with the smallest-stake series.

    Ratio > 1 means Option 3 needs more bandwidth; < 1 means Option 5 does.

    Args:
        N:                    Total nodes.
        M:                    Message throughput in MiB/s.
        max_stake_values_pct: Max-stake percentages to sweep.
        total_bps:            Total stake in basis points.
    """
    if max_stake_values_pct is None:
        max_stake_values_pct = [5.0, 10.0, 15.0, 20.0, 25.0, 32.0]

    # The fixed tier series we track.
    tracked_tiers_pct = [20.0, 15.0, 10.0, 5.0]

    # {tier_label: [(max_stake_pct, ratio)]}  — only where the tier exists.
    series: dict[str, list[tuple[float, float]]] = {}
    # Also track the smallest-remainder tier (always exists).
    smallest_series: list[tuple[float, str, float]] = []

    # Per-distribution detail rows for the full tables.
    table_rows: list[tuple[float, list[tuple[str, float, float, float]]]] = []

    for pct in max_stake_values_pct:
        stakes = make_tiered_stakes(N, pct, total_bps=total_bps)

        wc3 = _per_validator_worst_case(
            stakes, M, option3_uploads, {"T": 2 * (N - 1)}
        )
        wc5 = _per_validator_worst_case(
            stakes, M, option5_uploads, {"T": 2 * N}
        )

        # Build stake -> first-index and count maps.
        first_idx: dict[int, int] = {}
        counts: dict[int, int] = {}
        for i, s in enumerate(stakes):
            if s not in first_idx:
                first_idx[s] = i
            counts[s] = counts.get(s, 0) + 1
        sorted_tiers = sorted(first_idx.keys(), reverse=True)

        # Full detail rows for this distribution.
        tiers: list[tuple[str, float, float, float]] = []
        for tier_bps in sorted_tiers:
            idx = first_idx[tier_bps]
            tier_pct = tier_bps / (total_bps / 100)
            label = f"{tier_pct:.2f}% ({counts[tier_bps]})"
            o3 = wc3[idx]
            o5 = wc5[idx]
            ratio = o3 / o5 if o5 > 0 else float("inf")
            tiers.append((label, o3, o5, ratio))
        table_rows.append((pct, tiers))

        # Track each fixed tier series.
        for tier_pct in tracked_tiers_pct:
            tier_bps = int(round(tier_pct / 100.0 * total_bps))
            if tier_bps not in first_idx:
                continue
            idx = first_idx[tier_bps]
            o5 = wc5[idx]
            if o5 > 0:
                ratio = wc3[idx] / o5
                label = f"{tier_pct:.0f}% stake"
                series.setdefault(label, []).append((pct, ratio))

        # Smallest tier.
        smallest_bps = min(first_idx.keys())
        smallest_pct_val = smallest_bps / (total_bps / 100)
        idx = first_idx[smallest_bps]
        o5 = wc5[idx]
        if o5 > 0:
            ratio = wc3[idx] / o5
            slabel = f"{smallest_pct_val:.2f}%"
            smallest_series.append((pct, slabel, ratio))

    # --- Heading ---
    print(f"## Option 3 vs Option 5: N = {N}, M = {M} MiB/s")
    print()
    print("Ratio = Opt\\_3 upload / Opt\\_5 upload. "
          "Values **> 1** mean Option 3 needs more bandwidth; "
          "**< 1** means Option 5 needs more.")

    # --- Per-distribution detail tables ---
    for pct, tiers in table_rows:
        print()
        print(f"### Max stake = {pct:.0f}%")
        print()
        print("| Tier (stake, count) | Opt 3 MiB/s | Opt 5 MiB/s | Ratio |")
        print("| :-- | --: | --: | --: |")
        for label, o3, o5, ratio in tiers:
            print(f"| {label} | {o3:.1f} | {o5:.1f} | {ratio:.2f} |")

    # --- Chart data summary table ---
    print()
    print("### Ratio summary (chart data)")
    print()

    all_labels = [l for l in series]
    header = ["Max stake"] + all_labels + ["smallest"]
    sep = ["--:"] * len(header)
    print("| " + " | ".join(header) + " |")
    print("| " + " | ".join(sep) + " |")
    for pct in max_stake_values_pct:
        row = [f"{pct:.0f}%"]
        for lbl in all_labels:
            pts = series[lbl]
            match = [p for p in pts if p[0] == pct]
            row.append(f"{match[0][1]:.2f}" if match else "-")
        smatch = [p for p in smallest_series if p[0] == pct]
        row.append(f"{smatch[0][2]:.2f}" if smatch else "-")
        print("| " + " | ".join(row) + " |")

    # --- Dense Mermaid charts (Gbps) ---
    # For each tracked tier, sweep max_stake from that tier's % up to
    # max_pct_limit in 1% steps and plot Opt 3 and Opt 5 bandwidth in Gbps.

    max_pct_limit = 32.0
    import sys

    def _gbps_for_tier(
        tier_pct: float, max_stake_pct: float,
    ) -> tuple[float, float, float, float] | None:
        """Compute Opt3 and Opt5 Gbps for a tier at a given max_stake distribution.

        Returns (tier_opt3_gbps, tier_opt5_gbps,
                 smallest_opt3_gbps, smallest_opt5_gbps)
        or None if tier doesn't exist.
        """
        stakes = make_tiered_stakes(N, max_stake_pct, total_bps=total_bps)
        tier_bps = int(round(tier_pct / 100.0 * total_bps))

        first_idx_map: dict[int, int] = {}
        for i, s in enumerate(stakes):
            if s not in first_idx_map:
                first_idx_map[s] = i

        if tier_bps not in first_idx_map:
            return None

        wc3 = _per_validator_worst_case(
            stakes, M, option3_uploads, {"T": 2 * (N - 1)}
        )
        wc5 = _per_validator_worst_case(
            stakes, M, option5_uploads, {"T": 2 * N}
        )

        idx = first_idx_map[tier_bps]
        t3 = _mib_to_gbps(wc3[idx])
        t5 = _mib_to_gbps(wc5[idx])

        smallest_bps = min(first_idx_map.keys())
        s_idx = first_idx_map[smallest_bps]
        s3 = _mib_to_gbps(wc3[s_idx])
        s5 = _mib_to_gbps(wc5[s_idx])

        return (t3, t5, s3, s5)

    # Build dense series for each tracked tier.
    # {tier_pct: [(max_stake_pct, t_opt3, t_opt5, s_opt3, s_opt5), ...]}
    dense: dict[float, list[tuple[float, float, float, float, float]]] = {}
    for tier_pct in tracked_tiers_pct:
        start = tier_pct + 1.0
        if start > max_pct_limit:
            continue
        pts: list[tuple[float, float, float, float, float]] = []
        ms = start
        while ms <= max_pct_limit + 0.01:
            print(
                f"\r  charting {tier_pct:.0f}% tier @ max_stake={ms:.0f}% …",
                end="", file=sys.stderr,
            )
            result = _gbps_for_tier(tier_pct, ms)
            if result is not None:
                pts.append((ms, result[0], result[1], result[2], result[3]))
            ms += 1.0
        print(file=sys.stderr)
        if pts:
            dense[tier_pct] = pts

    # Colors for the four line series (applied in order by Mermaid).
    line_colors = ["#E63946", "#457B9D", "#E9C46A", "#2A9D8F"]
    palette_str = ", ".join(line_colors)

    print()
    print("### Bandwidth charts (Gbps)")
    print()
    print("> Each chart shows worst-case upload in Gbps for a given stake tier.")
    print("> Two lines per option: the tracked tier and the smallest-stake tier.")

    for tier_pct in tracked_tiers_pct:
        if tier_pct not in dense:
            continue
        pts = dense[tier_pct]
        label = f"{tier_pct:.0f}%"
        x_ticks = [p[0] for p in pts]
        t3_vals = [p[1] for p in pts]
        t5_vals = [p[2] for p in pts]
        s3_vals = [p[3] for p in pts]
        s5_vals = [p[4] for p in pts]

        x_labels = ", ".join(f'"{x:.0f}"' for x in x_ticks)

        all_vals = t3_vals + t5_vals + s3_vals + s5_vals
        y_lo = min(all_vals)
        y_hi = max(all_vals)
        margin = (y_hi - y_lo) * 0.12 or 0.1

        series_labels = [
            f"Opt 3 {label} stake",
            f"Opt 5 {label} stake",
            "Opt 3 smallest stake",
            "Opt 5 smallest stake",
        ]

        print()
        print(f"**{label} stake tier** "
              f"(max stake {x_ticks[0]:.0f}%\u2013{x_ticks[-1]:.0f}%):")
        print()
        print("```mermaid")
        print("---")
        print("config:")
        print("    themeVariables:")
        print("        xyChart:")
        print(f'            plotColorPalette: "{palette_str}"')
        print("---")
        print("xychart-beta")
        print(f'    title "Worst-case upload: {label} and smallest stake (Gbps)"')
        print(f'    x-axis "Max Stake (%)" [{x_labels}]')
        print(f'    y-axis "Upload (Gbps)" {max(0, y_lo - margin):.2f} --> '
              f"{y_hi + margin:.2f}")
        print(f"    line [{', '.join(f'{v:.2f}' for v in t3_vals)}]")
        print(f"    line [{', '.join(f'{v:.2f}' for v in t5_vals)}]")
        print(f"    line [{', '.join(f'{v:.2f}' for v in s3_vals)}]")
        print(f"    line [{', '.join(f'{v:.2f}' for v in s5_vals)}]")
        print("```")
        print()
        print("| Color | Series |")
        print("|:---:|:---|")
        for color, slabel in zip(line_colors, series_labels):
            print(f"| $\\color{{{color}}}\\textsf{{\\textbf{{\u2501\u2501\u2501}}}}$ "
                  f"| {slabel} |")


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    run_sweep()
    print("\n---\n")
    run_per_validator_sweep()
    print("\n---\n")
    print_opt3_vs_opt5()


if __name__ == "__main__":
    main()
