# Hyperdisk Setup for Sequencer Sync

## Quick Summary

We needed to use Hyperdisk (Google's high-performance disk) with c4d instance types for better I/O performance. This required:

1. **Created Hyperdisk StorageClass** - Defined performance specs (10K IOPS, 140MB/s throughput)
2. **Created Hyperdisk PVC** - 100GB persistent volume for database storage
3. **Updated Job to use c4d nodes** - Added nodeSelector and tolerations to target c4d-standard-8 instances
4. **Fixed CPU requests** - Reduced from 8 to 7 CPUs to leave room for system pods
5. **Fixed config loading** - Added all necessary config files to avoid P2P errors

## Problems We Hit & Fixes

### Error 1: Job is immutable
**Problem**: Can't update a running job's nodeSelector  
**Fix**: Delete the old job first, then apply the new one

### Error 2: Insufficient CPU
**Problem**: Pod stuck pending - c4d-standard-8 has 8 vCPUs but system uses some  
**Fix**: Changed CPU request from 8 to 7

### Error 3: Disk attached to old pod
**Problem**: "Multi-Attach error" - Hyperdisk can only attach to one pod at a time  
**Fix**: Delete the old completed job to free the disk

### Error 4: Missing P2P config
**Problem**: "missing field `bootstrap_peer_multiaddr`"  
**Fix**: Load ALL config files including mainnet_deployment, mainnet_hybrid, node_config, mainnet_secrets.json

---

## Detailed Documentation

### What We Built

#### 1. Hyperdisk StorageClass (`hyperdisk-balanced-100gb.yaml`)
```yaml
apiVersion: storage.k8s.io/v1
kind: StorageClass
metadata:
  name: hyperdisk-balanced-100gb
parameters:
  type: hyperdisk-balanced
  provisioned-iops-on-create: "10000"      # 10K IOPS
  provisioned-throughput-on-create: "140Mi" # 140 MB/s
provisioner: pd.csi.storage.gke.io
```

**Why**: Hyperdisk Balanced offers much better I/O performance than standard pd-balanced disks. We provision specific IOPS and throughput upfront.

#### 2. Hyperdisk PVC (`pvc-hyperdisk.yaml`)
```yaml
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: sequencer-database-hyperdisk
  namespace: batching-test
spec:
  accessModes:
    - ReadWriteOnce
  storageClassName: hyperdisk-balanced-100gb
  resources:
    requests:
      storage: 100Gi
```

**Why**: This creates the actual 100GB disk using our StorageClass. The disk must match the minimum size for the provisioned IOPS.

#### 3. Updated Test Job (`test-job.yaml`)
Added:
```yaml
spec:
  template:
    spec:
      nodeSelector:
        role: "apollo-core-service-c4d-standard-8"
      tolerations:
        - key: key
          operator: "Equal"
          value: "apollo-core-service-c4d-standard-8"
          effect: "NoSchedule"
```

**Why**: Hyperdisk requires c4d (4th gen) instances. The nodeSelector targets our c4d node pool, and tolerations allow scheduling on those tainted nodes.

Changed CPU request:
```yaml
resources:
  requests:
    cpu: "7"  # Was 8, but c4d-standard-8 needs room for system pods
```

Changed PVC reference:
```yaml
volumes:
  - name: database-storage
    persistentVolumeClaim:
      claimName: sequencer-database-hyperdisk  # Was sequencer-database-pvc
```

#### 4. Sync to 200k Job (`sync-to-200k.yaml`)
A dedicated job to sync the node to 200,000 blocks using the Hyperdisk. Key features:

- Uses same c4d nodeSelector/tolerations
- Loads all necessary configs (avoiding P2P errors)
- Creates inline test config to set storage path and enable batching
- Target: 200,000 blocks on Hyperdisk

Config loading order (critical!):
```bash
CONFIG_ARGS="--config_file /configs/base_layer_config.json"
CONFIG_ARGS="$CONFIG_ARGS --config_file /configs/batcher_config.json"
# ... all standard configs ...
CONFIG_ARGS="$CONFIG_ARGS --config_file /configs/mainnet_deployment"
CONFIG_ARGS="$CONFIG_ARGS --config_file /configs/mainnet_hybrid"
CONFIG_ARGS="$CONFIG_ARGS --config_file /configs/node_config"
CONFIG_ARGS="$CONFIG_ARGS --config_file /configs/mainnet_secrets.json"
CONFIG_ARGS="$CONFIG_ARGS --config_file test_config.json"
```

**Why this order matters**: The mainnet configs provide P2P settings that apollo_node expects. Without them, you get "missing field" errors.

#### 5. Resume SSD Sync Job (`resume-ssd-sync.yaml`)
A parallel job to resume syncing on the old SSD disk (doesn't affect Hyperdisk pod).

### Architecture

```
┌─────────────────────────────────────────┐
│  GKE Cluster (sequencer-dev)            │
│                                         │
│  ┌──────────────────────────────────┐  │
│  │ c4d-standard-8 Node              │  │
│  │                                  │  │
│  │  ┌────────────────────────────┐ │  │
│  │  │ sync-to-200k Pod           │ │  │
│  │  │ (7 CPUs, 16Gi RAM)         │ │  │
│  │  │                            │ │  │
│  │  │  apollo_node syncing...    │ │  │
│  │  └────────────┬───────────────┘ │  │
│  │               │                  │  │
│  │               │ mount            │  │
│  │               ▼                  │  │
│  │  ┌────────────────────────────┐ │  │
│  │  │ Hyperdisk PVC              │ │  │
│  │  │ 100GB, 10K IOPS            │ │  │
│  │  └────────────────────────────┘ │  │
│  └──────────────────────────────────┘  │
└─────────────────────────────────────────┘
```

### Key Learnings

1. **Hyperdisk + c4d are linked**: You MUST use c4d (4th gen) instances with Hyperdisk. Standard n2 instances won't work.

2. **Resource requests matter**: Even though c4d-standard-8 has 8 vCPUs, system pods (kube-proxy, fluentd, etc.) use some. Always leave 1 CPU free.

3. **Single-attach limitation**: Hyperdisk (like all GCP block storage) is ReadWriteOnce - only one pod can use it at a time. Clean up old pods before starting new ones.

4. **Config loading is fragile**: apollo_node expects certain configs to be loaded together. Missing mainnet_deployment or mainnet_secrets causes cryptic P2P errors.

5. **Cluster autoscaling works**: When we needed a c4d node in the right zone, GKE automatically scaled up a new node for us.

### How to Deploy

1. Create the StorageClass:
   ```bash
   kubectl apply -f hyperdisk-balanced-100gb.yaml
   ```

2. Create the PVC:
   ```bash
   kubectl apply -f pvc-hyperdisk.yaml
   ```

3. Run the sync job:
   ```bash
   kubectl apply -f sync-to-200k.yaml
   ```

4. Monitor progress:
   ```bash
   kubectl logs -f -n batching-test sync-to-200k-<pod-suffix>
   ```

### Monitoring I/O Performance

See I/O graphs in Google Cloud Console:
1. Go to: https://console.cloud.google.com/compute/disks?project=starkware-dev
2. Find disk: "sequencer-database-hyperdisk"
3. Click "OBSERVABILITY" tab
4. View IOPS, throughput, and operation metrics

### Files Changed/Added

- `hyperdisk-balanced-100gb.yaml` - NEW: StorageClass definition
- `pvc-hyperdisk.yaml` - NEW: Hyperdisk PVC
- `sync-to-200k.yaml` - NEW: 200k block sync job
- `resume-ssd-sync.yaml` - NEW: Resume old SSD sync
- `test-job.yaml` - MODIFIED: Updated for c4d nodes and Hyperdisk

Total: 4 new files, 1 modified file

