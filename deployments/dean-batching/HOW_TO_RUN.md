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
