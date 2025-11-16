## Files in This Directory

| File | Purpose |
|------|---------|
| `storage-class.yaml` | Defines Hyperdisk storage type (500GB capacity) |
| `pvc.yaml` | Creates the persistent disk (500GB Hyperdisk) |
| `batching-test.yaml` | Runs the batching comparison test |
| `HOW_TO_RUN.md` | This guide |

---

## Prerequisites

### 1. **You must have kubectl access to the cluster:**
```bash
gcloud auth login
gcloud container clusters get-credentials sequencer-dev --region us-central1 --project starkware-dev
```

### 2. **Verify you can connect:**
```bash
kubectl cluster-info
```

You should see: `Kubernetes control plane is running at https://...`

---

## Step-by-Step Instructions

### **Step 1: Navigate to the directory**

```bash
cd /home/dean/workspace/sequencer/deployments/dean-batching
```

---

### **Step 2: Create the StorageClass (Hyperdisk type)**

This defines **how** Kubernetes creates Hyperdisks.

```bash
kubectl apply -f storage-class.yaml
```

**Expected output:**
```
storageclass.storage.k8s.io/hyperdisk-balanced-500gb created
```

---

### **Step 3: Create the PersistentVolumeClaim (request the disk)**

This **actually creates** the 500GB Hyperdisk.

```bash
kubectl apply -f pvc.yaml
```

**Expected output:**
```
persistentvolumeclaim/dean-hyperdisk-pvc created
```

**Check the PVC status:**
```bash
kubectl get pvc -n dean-batching
```

You should see:
```
NAME                  STATUS    VOLUME   CAPACITY   STORAGECLASS              AGE
dean-hyperdisk-pvc    Pending   -        -          hyperdisk-balanced-500gb  10s
```

**NOTE:** Status will be `Pending` until a pod uses it (this is normal with `WaitForFirstConsumer`).

---

### **Step 4: Create the namespace (if needed)**

If the namespace doesn't exist:

```bash
kubectl create namespace dean-batching
```

---

### **Step 5: Create ConfigMap with node configs**

The test needs configuration files. Check if they exist:

```bash
kubectl get configmap sequencer-configs -n dean-batching
```

**If they DON'T exist, create them:**

This requires the `deploy_batching_test.sh` script from the old directory. You can either:

**Option A: Copy and run just the ConfigMap creation part:**

```bash
cd /home/dean/workspace/sequencer/deployments/batching-test
bash -c '
    source deploy_batching_test.sh
    NAMESPACE="dean-batching"
    create_config_files_configmap
'
```

**Option B: Manually check if they exist in the batching-test namespace:**

```bash
# Check if configs exist in old namespace
kubectl get configmap sequencer-configs -n batching-test

# If yes, copy them:
kubectl get configmap sequencer-configs -n batching-test -o yaml | \
  sed 's/namespace: batching-test/namespace: dean-batching/' | \
  kubectl apply -f -
```

---

### **Step 6: Deploy the test job**

```bash
kubectl apply -f batching-test.yaml
```

**Expected output:**
```
job.batch/dean-batching-test created
```

---

### **Step 7: Monitor the test progress**

**Check if the pod is running:**

```bash
kubectl get pods -n dean-batching
```

You should see:
```
NAME                         READY   STATUS    RESTARTS   AGE
dean-batching-test-xxxxx     1/1     Running   0          30s
```

**Watch the logs in real-time:**

```bash
kubectl logs -f -n dean-batching -l app=dean-batching-test
```

You'll see output like:

```
=========================================
DEAN'S BATCHING TEST: 0 → 50K
=========================================

=== TEST 1: WITHOUT BATCHING ===
Starting sync WITHOUT batching from block 0...
PID: 24
Monitoring progress (checking every 1 second)...
  Block 100 (5s elapsed)
  Block 250 (10s elapsed)
  Block 450 (15s elapsed)
  ...
  ✅ Reached target! Stopping at block 50001...

=== RESULT 1: WITHOUT BATCHING ===
  Blocks synced: 50001
  Time: 1834s (30m 34s)
  Speed: 27.27 blocks/sec
  Disk usage: 8.2G

Cleaning up for next test...

=== TEST 2: WITH BATCHING ===
Starting sync WITH batching from block 0...
PID: 3425
Monitoring progress (checking every 1 second)...
  Block 150 (5s elapsed)
  Block 320 (10s elapsed)
  Block 510 (15s elapsed)
  ...
  Reached target! Stopping at block 50002...

=== RESULT 2: WITH BATCHING ===
  Blocks synced: 50002
  Time: 1245s (20m 45s)
  Speed: 40.16 blocks/sec
  Disk usage: 8.3G

=========================================
FINAL COMPARISON
=========================================

WITHOUT Batching:
  Blocks: 50001
  Time: 1834s
  Speed: 27.27 blocks/sec

WITH Batching:
  Blocks: 50002
  Time: 1245s
  Speed: 40.16 blocks/sec

WITH batching is 1.47x FASTER
   Speed improvement: 1.47x

NOTE: Blocks 0-50k are very small (~80 bytes each)
Batching may add overhead on tiny blocks
For fair comparison, test blocks 200k+ with real transactions

=========================================
TEST COMPLETE
=========================================
```

---

### **Step 8: Check the results**

After the test completes (20-60 minutes), review the final comparison:

```bash
kubectl logs -n dean-batching -l app=dean-batching-test | tail -50
```

---

## How to Stop or Delete the Test

**Stop the running job:**

```bash
kubectl delete job dean-batching-test -n dean-batching
```

**Delete the disk (WARNING: This deletes all data!):**

```bash
kubectl delete pvc dean-hyperdisk-pvc -n dean-batching
```

**Delete the entire namespace (everything):**

```bash
kubectl delete namespace dean-batching
```

---

## Troubleshooting

### **Problem: Pod is stuck in "Pending" state**

```bash
kubectl describe pod -n dean-batching -l app=dean-batching-test
```

**Common causes:**
- **No c4d nodes available:** Wait 2-5 minutes for autoscaler to create nodes
- **PVC not bound:** Check `kubectl get pvc -n dean-batching`
- **Resource limits:** c4d-standard-8 has 8 vCPUs, pod requests 7 (leaving 1 for system)

---

### **Problem: "Error from server (Unauthorized)"**

Your kubectl credentials expired. Re-authenticate:

```bash
gcloud auth login
gcloud container clusters get-credentials sequencer-dev --region us-central1 --project starkware-dev
```

---

### **Problem: ConfigMap not found**

The test needs `sequencer-configs` ConfigMap. Follow Step 5 above to create it.

---

### **Problem: Test syncs too many blocks (goes past 50k)**

Early blocks (0-50k) are very small and sync extremely fast. The monitoring script checks every 1 second, but hundreds of blocks can sync in that time.

**This is expected behavior for early blocks!**

For more accurate testing, test blocks 200k-250k where blocks contain real transaction data.

---

## Understanding the Results

### **What the numbers mean:**

- **Blocks synced:** How many blocks were written to the database
- **Time:** Total sync duration in seconds (and minutes)
- **Speed:** Blocks per second (blocks/time)
- **Disk usage:** Size of the database directory

### **Interpreting the comparison:**

- **1.5x faster:** Batching is 50% faster
- **2.0x faster:** Batching is 100% faster (twice as fast)
- **0.8x faster:** Batching is 20% slower (not faster!)

### **Important note about blocks 0-50k:**

These blocks are mostly **empty** (80 bytes each, no transactions). They download extremely fast from the network, so:
- Batching may appear slower due to overhead
- Network speed dominates over disk I/O
- **Fair comparison requires blocks 200k+ with real transactions**

---

## Summary: Quick Commands

```bash
# 1. Navigate to directory
cd /home/dean/workspace/sequencer/deployments/dean-batching

# 2. Create everything (StorageClass, PVC, Namespace, Job)
kubectl apply -f storage-class.yaml
kubectl apply -f pvc.yaml
kubectl create namespace dean-batching  # if needed
# (Create ConfigMap - see Step 5)
kubectl apply -f batching-test.yaml

# 3. Watch the test
kubectl logs -f -n dean-batching -l app=dean-batching-test

# 4. Check results
kubectl logs -n dean-batching -l app=dean-batching-test | tail -50

# 5. Clean up (delete everything)
kubectl delete job dean-batching-test -n dean-batching
kubectl delete pvc dean-hyperdisk-pvc -n dean-batching
kubectl delete storageclass hyperdisk-balanced-500gb
```

---
