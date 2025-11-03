# Volume Configuration Guide

This document describes all available configuration options for the Volume construct.

## Basic Configuration

```yaml
persistentVolume:
  enabled: false
  size: "10Gi"
  storageClass: ""
  accessModes:
    - "ReadWriteOnce"
  annotations: {}
  labels: {}
```

## Advanced Configuration

```yaml
persistentVolume:
  enabled: true
  size: "100Gi"
  storageClass: "fast-ssd"
  accessModes:
    - "ReadWriteOnce"
  annotations:
    "volume.beta.kubernetes.io/storage-class": "fast-ssd"
  labels:
    "app.kubernetes.io/component": "storage"
  volumeMode: "Filesystem"
  dataSource: {}
  dataSourceRef: {}
  selector: {}
  resources:
    requests:
      storage: "100Gi"
```

## Configuration Options

### `enabled` (boolean)
- **Default**: `false`
- **Description**: Whether to create the PersistentVolumeClaim resource
- **Example**: `enabled: true` to enable persistent volume creation

### `size` (string)
- **Default**: `"10Gi"`
- **Description**: Size of the persistent volume
- **Example**: `size: "100Gi"` for 100 gigabytes

### `storageClass` (string, optional)
- **Default**: `""`
- **Description**: Storage class for the persistent volume
- **Example**: `storageClass: "fast-ssd"` for SSD storage

### `accessModes` (array of strings)
- **Default**: `["ReadWriteOnce"]`
- **Description**: Access modes for the persistent volume
- **Values**: `"ReadWriteOnce"`, `"ReadOnlyMany"`, `"ReadWriteMany"`
- **Example**: `accessModes: ["ReadWriteOnce"]` for single node access

### `annotations` (object)
- **Default**: `{}`
- **Description**: Annotations to add to the PersistentVolumeClaim metadata
- **Example**:
  ```yaml
  annotations:
    "volume.beta.kubernetes.io/storage-class": "fast-ssd"
    "backup.kubernetes.io/enabled": "true"
  ```

### `labels` (object)
- **Default**: `{}`
- **Description**: Labels to add to the PersistentVolumeClaim metadata
- **Example**:
  ```yaml
  labels:
    "app.kubernetes.io/component": "storage"
    "backup.kubernetes.io/backup": "true"
  ```

### `volumeMode` (string, optional)
- **Default**: `"Filesystem"`
- **Description**: Volume mode for the persistent volume
- **Values**: `"Filesystem"` or `"Block"`

### `dataSource` (object, optional)
- **Default**: `{}`
- **Description**: Data source for the persistent volume
- **Example**:
  ```yaml
  dataSource:
    name: "source-pvc"
    kind: "PersistentVolumeClaim"
  ```

### `dataSourceRef` (object, optional)
- **Default**: `{}`
- **Description**: Data source reference for the persistent volume
- **Example**:
  ```yaml
  dataSourceRef:
    name: "source-pvc"
    kind: "PersistentVolumeClaim"
  ```

### `selector` (object, optional)
- **Default**: `{}`
- **Description**: Label selector for the persistent volume
- **Example**:
  ```yaml
  selector:
    matchLabels:
      "storage-type": "ssd"
  ```

### `resources` (object, optional)
- **Default**: `{}`
- **Description**: Resource requirements for the persistent volume
- **Example**:
  ```yaml
  resources:
    requests:
      storage: "100Gi"
    limits:
      storage: "200Gi"
  ```

## Storage Class Examples

### AWS EBS

```yaml
persistentVolume:
  enabled: true
  size: "100Gi"
  storageClass: "gp3"
  accessModes:
    - "ReadWriteOnce"
  annotations:
    "volume.beta.kubernetes.io/storage-class": "gp3"
```

### GCP Persistent Disk

```yaml
persistentVolume:
  enabled: true
  size: "100Gi"
  storageClass: "pd-ssd"
  accessModes:
    - "ReadWriteOnce"
  annotations:
    "volume.beta.kubernetes.io/storage-class": "pd-ssd"
```

### Azure Disk

```yaml
persistentVolume:
  enabled: true
  size: "100Gi"
  storageClass: "managed-premium"
  accessModes:
    - "ReadWriteOnce"
  annotations:
    "volume.beta.kubernetes.io/storage-class": "managed-premium"
```

## Access Mode Examples

### Single Node Access (Default)

```yaml
persistentVolume:
  enabled: true
  size: "100Gi"
  accessModes:
    - "ReadWriteOnce"
```

### Multi-Node Read-Only Access

```yaml
persistentVolume:
  enabled: true
  size: "100Gi"
  accessModes:
    - "ReadOnlyMany"
```

### Multi-Node Read-Write Access

```yaml
persistentVolume:
  enabled: true
  size: "100Gi"
  accessModes:
    - "ReadWriteMany"
  storageClass: "nfs"
```

## Volume Cloning

### Clone from Existing PVC

```yaml
persistentVolume:
  enabled: true
  size: "100Gi"
  storageClass: "fast-ssd"
  accessModes:
    - "ReadWriteOnce"
  dataSource:
    name: "source-pvc"
    kind: "PersistentVolumeClaim"
```

### Clone from Snapshot

```yaml
persistentVolume:
  enabled: true
  size: "100Gi"
  storageClass: "fast-ssd"
  accessModes:
    - "ReadWriteOnce"
  dataSource:
    name: "source-snapshot"
    kind: "VolumeSnapshot"
```

## Volume Expansion

### Expandable Volume

```yaml
persistentVolume:
  enabled: true
  size: "100Gi"
  storageClass: "expandable-ssd"
  accessModes:
    - "ReadWriteOnce"
  resources:
    requests:
      storage: "100Gi"
    limits:
      storage: "500Gi"
```

## Backup and Restore

### Backup-Enabled Volume

```yaml
persistentVolume:
  enabled: true
  size: "100Gi"
  storageClass: "backup-enabled"
  accessModes:
    - "ReadWriteOnce"
  annotations:
    "backup.kubernetes.io/enabled": "true"
    "backup.kubernetes.io/schedule": "0 2 * * *"
  labels:
    "backup.kubernetes.io/backup": "true"
```

## Generated Kubernetes Resource

The configuration above generates a PersistentVolumeClaim resource like this:

```yaml
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: sequencer-node-data
  namespace: default
  labels:
    app: sequencer
    service: sequencer-node
    app.kubernetes.io/component: storage
  annotations:
    volume.beta.kubernetes.io/storage-class: fast-ssd
    backup.kubernetes.io/enabled: "true"
spec:
  accessModes:
    - ReadWriteOnce
  resources:
    requests:
      storage: 100Gi
  storageClassName: fast-ssd
  volumeMode: Filesystem
```

## Mounting in Pods

The PersistentVolumeClaim can be mounted in pods using volume mounts:

```yaml
volumes:
  - name: data-volume
    persistentVolumeClaim:
      claimName: sequencer-node-data

volumeMounts:
  - name: data-volume
    mountPath: /app/data
```

## Best Practices

1. **Size Planning**: Plan for future growth when setting volume size
2. **Storage Class**: Choose appropriate storage class for your workload
3. **Access Modes**: Use ReadWriteOnce for single-node workloads
4. **Backup**: Enable backup for important data volumes
5. **Labels**: Use consistent labeling for better resource management
6. **Expansion**: Use expandable storage classes when possible
7. **Performance**: Choose storage class based on performance requirements
8. **Cost**: Consider cost implications of different storage classes
