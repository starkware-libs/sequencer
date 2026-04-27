# Storage Analyzer on K8s

Run the storage analyzer against a mainnet MDBX database on a k8s PVC.

## Steps

### 1. Deploy the pod

Edit `pod-analyzer.yaml` to match your namespace and PVC name, then:

```bash
kubectl apply -f deployments/images/storage_analyzer/pod-analyzer.yaml
kubectl wait --for=condition=Ready pod/storage-analyzer -n echonet-committer --timeout=120s
```

### 2. Build inside the pod

```bash
kubectl exec -it storage-analyzer -n echonet-committer -- bash

# Inside the pod:
apt-get update && apt-get install -y git clang pkg-config
git clone https://github.com/starkware-libs/sequencer.git --depth 1 --branch 03-29-apollo_storage_add_storage_analyzer /build
cd /build
cargo build --release --bin storage_analyzer --features storage_cli -p apollo_storage
```

### 3. Run the analyzer

```bash
# Still inside the pod. Adjust paths to match your PVC layout.

# Metadata only (instant):
/build/target/release/storage_analyzer --db-path /data/SN_MAIN

# Quick scan (seconds — unique key counts):
/build/target/release/storage_analyzer --db-path /data/SN_MAIN --scan quick

# Deep scan (minutes — full analysis with retention tradeoff table):
/build/target/release/storage_analyzer --db-path /data/SN_MAIN --scan deep

# Multiple DBs (batcher + sync):
/build/target/release/storage_analyzer \
    --db-path /data/batcher/SN_MAIN \
    --db-path /data/sync/SN_MAIN \
    --scan deep

# JSON output (for scripts):
/build/target/release/storage_analyzer --db-path /data/SN_MAIN --scan deep --format json > /tmp/analysis.json
```

### 4. Copy results out

```bash
# From your local machine:
kubectl cp echonet-committer/storage-analyzer:/tmp/analysis.json ./analysis.json
```

### 5. Cleanup

```bash
kubectl delete pod storage-analyzer -n echonet-committer
```

## Notes

- The PVC is mounted read-only — the analyzer only reads, never writes.
- The `rust:1.87-bookworm` image includes cargo and rustc. Build takes ~5-10 min first time.
- The `build-cache` emptyDir caches cargo registry downloads (lost on pod delete).
- If the sequencer is running and MDBX refuses concurrent access, stop the sequencer first
  or copy `mdbx.dat` + `mdbx.lck` to `/tmp/` inside the pod and point the analyzer there.
