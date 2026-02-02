# K8s Batching Test - File Explanation

## What We Have Now

I've copied and updated the k8s deployment files. Here's what each file does in simple terms:

---

## Files Created

### 1. `storage-class-balanced-1tb.yaml`
**What it is:** Tells Kubernetes what TYPE of disk to create  
**What it does:** Defines a "Hyperdisk Balanced" disk with:
- 80,000 IOPS (disk operations per second) - very fast!
- 1200 MB/s throughput - can read/write 1.2 GB per second
- 1TB size

**Think of it as:** A recipe that says "when someone asks for a disk, make it THIS fast and THIS big"

---

### 2. `pvc.yaml` (PersistentVolumeClaim)
**What it is:** Actually REQUESTS a disk from Google Cloud  
**What it does:** Says "I need a 1TB disk using the recipe from storage-class-balanced-1tb.yaml"

**Think of it as:** Ordering the disk. The storage class is the menu, this is placing the order.

---

### 3. `batching-test-500k.yaml`
**What it is:** The main test job definition  
**What it does:** Tells Kubernetes to:
1. Start a container (like a mini computer) with our sequencer code
2. Mount the 1TB disk to it
3. Run the test_batching.sh script inside
4. Sync 500,000 blocks WITH and WITHOUT batching
5. Compare the performance

**Key parts:**
- `image: ghcr.io/starkware-libs/sequencer/sequencer:dean-storage-batching-tests-PLACEHOLDER`
  - This is the Docker image (our compiled code in a package)
  - We need to build this and replace PLACEHOLDER with the actual version
  
- `BLOCKS_TO_SYNC: "500000"` - How many blocks to sync
- `BATCH_SIZE: "1000"` - Batch 1000 writes together

**Think of it as:** The instruction manual that says "run this program, give it this much CPU/memory, connect it to this disk"

---

### 4. `test_batching.sh`
**What it is:** The actual test script  
**What it does:**
1. Runs the sequencer node TWICE:
   - Once WITH batching (batch_size = 1000)
   - Once WITHOUT batching (batch_size = 1)
2. Measures how long each takes
3. Compares the results

**I updated it to use our new config:**
```json
{
  "state_sync_config.storage_config.batch_config.batch_size": 1000
}
```

**Think of it as:** The actual test that runs inside the container

---

### 5. `HOW_TO_RUN.md`
**What it is:** Step-by-step instructions  
**What it does:** Explains how to deploy everything to Google Cloud

---

## How It All Works Together

```
1. You apply storage-class-balanced-1tb.yaml
   → Google Cloud knows how to make fast disks

2. You apply pvc.yaml
   → Google Cloud creates a 1TB disk

3. You apply batching-test-500k.yaml
   → Kubernetes starts a container
   → Mounts the disk to it
   → Runs test_batching.sh inside
   
4. test_batching.sh runs
   → Syncs blocks WITH batching
   → Syncs blocks WITHOUT batching
   → Shows you which is faster
```

---

## What We Still Need To Do

1. **Build a Docker image** - Package our code so k8s can run it
2. **Update the image name** in batching-test-500k.yaml (replace PLACEHOLDER)
3. **Deploy to k8s** - Run the kubectl commands
4. **Watch the logs** - See the test results

---

## Simple Analogy

Think of it like ordering a pizza:

- **storage-class** = The menu (what types of pizzas exist)
- **pvc** = Your order ("I want a large pepperoni")
- **batching-test-500k.yaml** = Delivery instructions ("bring it to this address, ring the doorbell")
- **test_batching.sh** = What you do with the pizza (eat it and rate it)
- **Docker image** = The actual pizza (our compiled code)

Right now we have the menu and the order, but we need to make the actual pizza (build the Docker image) before we can deliver it!
