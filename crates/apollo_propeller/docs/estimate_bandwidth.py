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

    Shards are allocated proportionally to non-publisher stake among the N-1
    receivers.  The publisher is excluded from the stake distribution for shard
    allocation and the build threshold (S'/3).  The receive threshold is
    2S/3 - s_p.

    *T* is the total number of shards.  It does not affect the approximate
    bandwidth formulas (the T cancels out), but is validated for correctness
    (T >= N-1).

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

    # s_coal': minimum qualifying coalition stake among receivers (publisher excluded).
    receiver_stakes = [stakes[i] for i in range(n) if i != proposer]
    s_coal_prime = _compute_s_coal_subset(receiver_stakes, s_prime)
    s_coal_prime_frac = s_coal_prime / s_prime

    # Publisher upload:  M * S' / s_coal' = M / s_coal_prime_frac
    publisher_upload = message_size / s_coal_prime_frac

    uploads: list[float] = []
    for i in range(n):
        if i == proposer:
            uploads.append(publisher_upload)
        else:
            s_i_frac = stakes[i] / s_prime  # fraction of non-publisher stake
            # Receiver: (s_i / S') * (N-2) * M * S' / s_coal'
            #         = (s_i / S') * (N-2) * M / s_coal_prime_frac
            upload = s_i_frac * (n - 2) * message_size / s_coal_prime_frac
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

    Shards are allocated proportionally to stake for all N nodes (including the
    publisher).  The publisher broadcasts their own shards directly to all N-1
    receivers (no delegation).  Each receiver broadcasts their own shards to
    N-2 peers.

    Publisher upload = (S + s_p*(N-2)) * M / s_coal
                     = distribution of non-pub shards + gossip of own shards.
    Receiver upload  = s_i * (N-2) * M / s_coal.

    *T* is the total number of shards.  It does not affect the approximate
    bandwidth formulas (the T cancels out), but is validated for correctness
    (T >= N).

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

    # s_coal: minimum qualifying coalition stake (publisher-independent).
    s_coal = compute_s_coal(stakes)
    s_coal_frac = s_coal / total_stake

    s_p = stakes[proposer]
    s_p_frac = s_p / total_stake

    # Publisher upload:  (S + s_p*(N-2)) * M / s_coal
    #   = (1 + s_p_frac*(N-2)) * M / s_coal_frac
    publisher_upload = (1 + s_p_frac * (n - 2)) * message_size / s_coal_frac

    uploads: list[float] = []
    for i in range(n):
        if i == proposer:
            uploads.append(publisher_upload)
        else:
            s_i_frac = stakes[i] / total_stake
            # Receiver: s_i * (N-2) * M / s_coal
            upload = s_i_frac * (n - 2) * message_size / s_coal_frac
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


def _compute_expansion_for_option3(stakes: list[int], proposer: int) -> float:
    """Compute expansion factor for Option 3 with the given proposer."""
    total_stake = sum(stakes)
    n = len(stakes)
    s_p = stakes[proposer]
    s_prime = total_stake - s_p
    receiver_stakes = [stakes[i] for i in range(n) if i != proposer]
    s_coal_prime = _compute_s_coal_subset(receiver_stakes, s_prime)
    return s_prime / s_coal_prime


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
    """Print bandwidth estimates for each option across a range of max_stake cutoffs.

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

    print("=" * 110)
    print(f"Worst-case bandwidth sweep: N = {N}, M = {M} MiB/s")
    print(f"For each stake pool and option, every distinct node is tried as publisher.")
    print(f"The row shows the publisher choice that maximises the peak upload from any node.")
    print("=" * 110)

    for pct in max_stake_values_pct:
        max_bps = int(round(pct / 100.0 * total_bps))
        if max_bps < 1:
            max_bps = 1
        stakes = make_stakes(N, max_bps, total_bps)
        total = sum(stakes)

        # Describe distribution.
        unique: dict[int, int] = {}
        for s in stakes:
            unique[s] = unique.get(s, 0) + 1
        parts = [f"{v}×{s/100:.2f}%" for s, v in sorted(unique.items(), reverse=True)]
        dist_str = "  +  ".join(parts)

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

        def _exp_option5(_stakes: list[int], _proposer: int) -> float:
            t = sum(_stakes)
            sc = compute_s_coal(_stakes)
            return t / sc

        # --- Option 1 (publisher-independent) ---
        wc1 = _worst_case_across_publishers(
            stakes, M, option1_uploads, expansion_fn=_exp_option1,
        )

        # --- Option 2 (publisher-dependent expansion) ---
        wc2 = _worst_case_across_publishers(
            stakes, M, option2_uploads,
            expansion_fn=_compute_expansion_for_option2,
        )

        # --- Option 3 (publisher-dependent expansion) ---
        wc3 = _worst_case_across_publishers(
            stakes, M, option3_uploads, {"T": N - 1},
            expansion_fn=_compute_expansion_for_option3,
        )

        # --- Option 4 (publisher-independent expansion) ---
        wc4 = _worst_case_across_publishers(
            stakes, M, option4_uploads, expansion_fn=_exp_option4,
        )

        # --- Option 5 (publisher-independent expansion) ---
        wc5 = _worst_case_across_publishers(
            stakes, M, option5_uploads, {"T": N},
            expansion_fn=_exp_option5,
        )

        def _pub_label(idx: int) -> str:
            return f"{stakes[idx]/100:.2f}%"

        def _mib_to_gbps(mib_per_s: float) -> float:
            """Convert MiB/s to Gbps (1 MiB = 2^20 bytes = 8 × 2^20 bits)."""
            return mib_per_s * 8 * (1024 * 1024) / 1_000_000_000

        print(f"\n--- max_stake = {pct:.1f}%  |  {dist_str}")
        print(f"    {'':20} {'expansion':>10} {'peak node':>11} "
              f"{'peak Gbps':>10} {'pub upload':>11} {'max rcv':>11}"
              f"  worst publisher")
        for label, wc in [
            ("Option 1 (nodes)",    wc1),
            ("Option 2 (excl fix)", wc2),
            ("Option 3 (excl prop)",wc3),
            ("Option 4 (pool fix)", wc4),
            ("Option 5 (pool prop)",wc5),
        ]:
            # wc = (peak_upload, pub_upload, max_rcv, expansion, publisher_idx)
            peak_gbps = _mib_to_gbps(wc[0])
            print(f"    {label:20} {wc[3]:10.2f}× {wc[0]:10.1f} "
                  f"{peak_gbps:10.2f} {wc[1]:10.1f}  {wc[2]:10.1f}  "
                  f"pub={_pub_label(wc[4])}")


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
    """Print per-validator worst-case bandwidth for each option and distribution.

    For each stake distribution, builds a tiered stake pool:
      [max, max, …, max, 20%, 15%, 10%, 5%, remainder]
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

    print("=" * 120)
    print(f"Per-validator worst-case bandwidth: N = {N}, M = {M} MiB/s")
    print(f"For each validator stake tier, shows the worst upload across ALL possible publishers.")
    print(f"This is the bandwidth each validator must provision.")
    print(f"Pool: [max, max, …, max, 20%, 15%, 10%, 5%, remainder] (tiers < max_stake only).")
    print("=" * 120)

    option_defs = [
        ("Opt 1 (nodes)",     option1_uploads, {}),
        ("Opt 2 (excl fix)",  option2_uploads, {}),
        ("Opt 3 (excl prop)", option3_uploads, {"T": N - 1}),
        ("Opt 4 (pool fix)",  option4_uploads, {}),
        ("Opt 5 (pool prop)", option5_uploads, {"T": N}),
    ]

    for pct in max_stake_values_pct:
        stakes = make_tiered_stakes(N, pct, total_bps=total_bps)

        # Describe distribution.
        unique: dict[int, int] = {}
        for s in stakes:
            unique[s] = unique.get(s, 0) + 1
        parts = [f"{v}×{s/100:.2f}%" for s, v in sorted(unique.items(), reverse=True)]
        dist_str = "  +  ".join(parts)

        # Identify distinct stake tiers (sorted descending).
        tiers = sorted(unique.keys(), reverse=True)

        # Compute per-validator worst-case for each option.
        option_results: list[tuple[str, list[float]]] = []
        for label, fn, kwargs in option_defs:
            wc = _per_validator_worst_case(stakes, M, fn, kwargs)
            option_results.append((label, wc))

        # Print header.
        print(f"\n{'─' * 120}")
        print(f"max_stake = {pct:.1f}%  |  {dist_str}")
        print(f"{'─' * 120}")

        # Build column headers: one column per stake tier.
        tier_headers = [f"{s/100:.2f}% ({unique[s]})" for s in tiers]

        # Print table header.
        col_w = 16  # column width for each tier
        opt_w = 20   # option label width
        print(f"{'':>{opt_w}}", end="")
        for th in tier_headers:
            print(f"{th:>{col_w}}", end="")
        print()

        # Print separator.
        print(f"{'':>{opt_w}}", end="")
        for _ in tiers:
            print(f"{'─' * (col_w - 1):>{col_w}}", end="")
        print()

        # For each option, print the worst-case upload per tier.
        for label, wc in option_results:
            print(f"{label:>{opt_w}}", end="")
            for s in tiers:
                # Find one representative index for this tier.
                idx = next(i for i, st in enumerate(stakes) if st == s)
                val_mib = wc[idx]
                print(f"{val_mib:>7.1f} MiB/s", end="   ")
            print()

        # Print Gbps row for each option.
        print()
        print(f"{'(in Gbps)':>{opt_w}}", end="")
        for _ in tiers:
            print(f"{'':>{col_w}}", end="")
        print()

        for label, wc in option_results:
            print(f"{label:>{opt_w}}", end="")
            for s in tiers:
                idx = next(i for i, st in enumerate(stakes) if st == s)
                val_gbps = _mib_to_gbps(wc[idx])
                print(f"{val_gbps:>7.2f} Gbps ", end="   ")
            print()


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    run_sweep()
    print("\n\n")
    run_per_validator_sweep()


if __name__ == "__main__":
    main()
