# Blockifier ERC20 Transfer Simulation Benchmark Results

## Test Configuration
- **Transaction**: Multicall with 10 ERC20 transfers to different recipients
- **Execution Mode**: Skip validation, skip fee charge (simulation flags)
- **Network**: Starknet Mainnet

---

## Results Summary

### Pathfinder RPC (`starknet_simulateTransactions`)

| Scenario | Time | Notes |
|----------|------|-------|
| **Cluster → Pathfinder RPC** | **~50 ms** | Same cluster, low latency |
| **Laptop → Pathfinder RPC** | **~5 s** | Remote call over internet |

### Direct Blockifier Execution (via `blockifier_reexecution`)

| Scenario | Time | Notes |
|----------|------|-------|
| **1. Laptop → Remote RPC State** | ~21.5 s | Fetching state over network |
| **2. Cluster - Cold Class Cache** | ~2.1 s | First execution, needs to compile Sierra → CASM |
| **3. Cluster - Warm Class, Cold State** | **319.65 ms** | Classes cached, fresh state from RPC |
| **4. Cluster - Warm Class, Warm State** | **18.81 ms** | Both caches warm (sequential tx) |

---

## Detailed Breakdown

### Pathfinder RPC Simulation (`starknet_simulateTransactions`)

#### Cluster → Pathfinder RPC
```
Environment: K8s pod calling pathfinder in same cluster
Time: ~50 ms
```
Full RPC simulation including pathfinder overhead.

#### Laptop → Pathfinder RPC  
```
Environment: Developer laptop calling pathfinder over internet
Time: ~5 seconds
```
Network latency dominates the response time.

---

### Direct Blockifier Execution

### 1. Laptop → Remote RPC State
```
Environment: Developer laptop → pathfinder at cluster
Execution time (warm class cache): ~21.5 seconds
```
⚠️ High latency due to network overhead between benchmark and RPC node.

### 2. Cluster - Cold Class Cache (First Execution)
```
Environment: K8s pod in same cluster as pathfinder
Time: ~2.1 - 3.3 seconds
```
Includes Sierra to CASM compilation for account + ERC20 contracts.

### 3. Cluster - Warm Class Cache, Cold State (tx1)
```
Environment: K8s pod, classes pre-compiled and cached
Mean: 319.65 ms
Range: [314.10 ms - 326.68 ms]
Per-transfer: ~32 ms
```
State is fetched from RPC for each storage slot.

### 4. Cluster - Warm Class + Warm State (tx2)
```
Environment: K8s pod, same executor as tx1
Mean: 18.81 ms
Range: [17.99 ms - 19.75 ms]
Per-transfer: ~1.9 ms
```
Benefiting from both class cache AND state cache (same storage slots as tx1).

---

## Key Insights

| Comparison | Speedup |
|------------|---------|
| Laptop → Cluster (RPC simulation) | **100x faster** (5s → 50ms) |
| Laptop → Cluster (blockifier direct) | **67x faster** (21.5s → 320ms) |
| Cold State → Warm State | **17x faster** (320ms → 19ms) |
| Cold Class → Warm Class | **6.5x faster** (2.1s → 320ms) |
| Pathfinder RPC vs Blockifier (cluster) | **~2.6x faster** via RPC (50ms vs 320ms)* |

*Note: Pathfinder RPC likely has warm caches from prior requests.

### Takeaways
1. **Network latency dominates** when calling from laptop (~5s RPC, ~21s blockifier)
2. **In-cluster performance** is excellent: 50ms (RPC) / 320ms (blockifier cold state) / 19ms (blockifier warm state)
3. **State caching provides 17x speedup** for sequential transactions accessing same storage
4. **Class compilation overhead** is ~2s for first execution, amortized across subsequent calls
5. **Per-transfer cost** with warm caches: ~1.9ms

---

