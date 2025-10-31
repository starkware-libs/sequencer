# StatefulSet Configuration Guide

This document describes all available configuration options for the StatefulSet construct.

## Basic Configuration

```yaml
statefulSet:
  enabled: true
  replicas: 1
  updateStrategy:
    type: "RollingUpdate"
  podManagementPolicy: "OrderedReady"
  serviceName: "sequencer-node-service"
  podAnnotations: {}
  serviceAccount:
    name: ""
    create: true
  securityContext: {}
  terminationGracePeriodSeconds: 300
```

## Advanced Configuration

```yaml
statefulSet:
  enabled: true
  replicas: 3
  updateStrategy:
    type: "RollingUpdate"
    rollingUpdate:
      maxUnavailable: 1
  podManagementPolicy: "OrderedReady"
  serviceName: "sequencer-node-service"
  podAnnotations:
    "prometheus.io/scrape": "true"
    "prometheus.io/port": "9090"
  serviceAccount:
    name: "sequencer-sa"
    create: true
  securityContext:
    runAsUser: 1000
    runAsNonRoot: true
    runAsGroup: 1000
    fsGroup: 1000
  terminationGracePeriodSeconds: 300
  nodeSelector:
    "node-type": "compute"
  tolerations:
    - key: "node-role.kubernetes.io/master"
      operator: "Exists"
      effect: "NoSchedule"
  affinity:
    nodeAffinity:
      requiredDuringSchedulingIgnoredDuringExecution:
        nodeSelectorTerms:
          - matchExpressions:
              - key: "node-type"
                operator: "In"
                values: ["compute"]
```

## Configuration Options

### `enabled` (boolean)
- **Default**: `true`
- **Description**: Whether to create the StatefulSet resource
- **Example**: `enabled: false` to disable StatefulSet creation

### `replicas` (integer)
- **Default**: `1`
- **Description**: Number of replicas to maintain
- **Example**: `replicas: 3` for high availability

### `updateStrategy` (object)
- **Default**: `{"type": "RollingUpdate"}`
- **Description**: Update strategy for the StatefulSet
- **Properties**:
  - `type` (string): Update strategy type ("RollingUpdate" or "OnDelete")
  - `rollingUpdate` (object, optional): Rolling update configuration
    - `maxUnavailable` (integer): Maximum number of unavailable pods during update

### `podManagementPolicy` (string)
- **Default**: `"OrderedReady"`
- **Description**: Pod management policy
- **Values**: `"OrderedReady"` or `"Parallel"`

### `serviceName` (string)
- **Default**: `"{service-name}-service"`
- **Description**: Name of the headless service for the StatefulSet
- **Example**: `serviceName: "sequencer-node-service"`

### `podAnnotations` (object)
- **Default**: `{}`
- **Description**: Annotations to add to pod metadata
- **Example**:
  ```yaml
  podAnnotations:
    "prometheus.io/scrape": "true"
    "prometheus.io/port": "9090"
    "sidecar.istio.io/inject": "true"
  ```

### `serviceAccount` (object)
- **Description**: Service account configuration for pods
- **Properties**:
  - `name` (string): Service account name
  - `create` (boolean): Whether to create the service account

### `securityContext` (object)
- **Default**: `{}`
- **Description**: Security context for pods
- **Properties**:
  - `runAsUser` (integer): User ID to run the container
  - `runAsNonRoot` (boolean): Whether to run as non-root user
  - `runAsGroup` (integer): Group ID to run the container
  - `fsGroup` (integer): File system group ID

### `terminationGracePeriodSeconds` (integer)
- **Default**: `300`
- **Description**: Grace period for pod termination
- **Example**: `terminationGracePeriodSeconds: 30` for quick shutdown

### `nodeSelector` (object, optional)
- **Default**: `{}`
- **Description**: Node selector for pod placement
- **Example**:
  ```yaml
  nodeSelector:
    "node-type": "compute"
    "zone": "us-west1-a"
  ```

### `tolerations` (array, optional)
- **Default**: `[]`
- **Description**: Tolerations for pod scheduling
- **Example**:
  ```yaml
  tolerations:
    - key: "node-role.kubernetes.io/master"
      operator: "Exists"
      effect: "NoSchedule"
    - key: "dedicated"
      operator: "Equal"
      value: "sequencer"
      effect: "NoSchedule"
  ```

### `affinity` (object, optional)
- **Default**: `{}`
- **Description**: Affinity rules for pod scheduling
- **Example**:
  ```yaml
  affinity:
    nodeAffinity:
      requiredDuringSchedulingIgnoredDuringExecution:
        nodeSelectorTerms:
          - matchExpressions:
              - key: "node-type"
                operator: "In"
                values: ["compute"]
    podAntiAffinity:
      preferredDuringSchedulingIgnoredDuringExecution:
        - weight: 100
          podAffinityTerm:
            labelSelector:
              matchExpressions:
                - key: "app"
                  operator: "In"
                  values: ["sequencer"]
            topologyKey: "kubernetes.io/hostname"
  ```

## Update Strategy Examples

### Rolling Update (Default)

```yaml
statefulSet:
  enabled: true
  replicas: 3
  updateStrategy:
    type: "RollingUpdate"
    rollingUpdate:
      maxUnavailable: 1
```

### OnDelete Strategy

```yaml
statefulSet:
  enabled: true
  replicas: 3
  updateStrategy:
    type: "OnDelete"
```

## Security Configuration

### Non-Root User

```yaml
statefulSet:
  enabled: true
  securityContext:
    runAsUser: 1000
    runAsNonRoot: true
    runAsGroup: 1000
    fsGroup: 1000
```

### Pod Security Standards

```yaml
statefulSet:
  enabled: true
  securityContext:
    runAsNonRoot: true
    runAsUser: 1000
    seccompProfile:
      type: "RuntimeDefault"
    capabilities:
      drop:
        - "ALL"
```

## High Availability Configuration

### Multi-Replica with Anti-Affinity

```yaml
statefulSet:
  enabled: true
  replicas: 3
  podManagementPolicy: "OrderedReady"
  affinity:
    podAntiAffinity:
      requiredDuringSchedulingIgnoredDuringExecution:
        - labelSelector:
            matchExpressions:
              - key: "app"
                operator: "In"
                values: ["sequencer"]
          topologyKey: "kubernetes.io/hostname"
```

### Zone Distribution

```yaml
statefulSet:
  enabled: true
  replicas: 6
  affinity:
    podAntiAffinity:
      preferredDuringSchedulingIgnoredDuringExecution:
        - weight: 100
          podAffinityTerm:
            labelSelector:
              matchExpressions:
                - key: "app"
                  operator: "In"
                  values: ["sequencer"]
            topologyKey: "topology.kubernetes.io/zone"
```

## Resource Management

### Resource Requests and Limits

```yaml
statefulSet:
  enabled: true
  replicas: 3
  resources:
    requests:
      memory: "1Gi"
      cpu: "500m"
    limits:
      memory: "2Gi"
      cpu: "1000m"
```

### Node Selection

```yaml
statefulSet:
  enabled: true
  replicas: 3
  nodeSelector:
    "node-type": "compute"
    "instance-type": "c5.xlarge"
  tolerations:
    - key: "dedicated"
      operator: "Equal"
      value: "sequencer"
      effect: "NoSchedule"
```

## Generated Kubernetes Resource

The configuration above generates a StatefulSet resource like this:

```yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: sequencer-node-statefulset
  namespace: default
spec:
  serviceName: sequencer-node-service
  replicas: 3
  podManagementPolicy: OrderedReady
  updateStrategy:
    type: RollingUpdate
    rollingUpdate:
      maxUnavailable: 1
  selector:
    matchLabels:
      app: sequencer
      service: sequencer-node
  template:
    metadata:
      labels:
        app: sequencer
        service: sequencer-node
      annotations:
        prometheus.io/scrape: "true"
        prometheus.io/port: "9090"
    spec:
      serviceAccountName: sequencer-sa
      securityContext:
        runAsUser: 1000
        runAsNonRoot: true
        runAsGroup: 1000
        fsGroup: 1000
      terminationGracePeriodSeconds: 300
      nodeSelector:
        node-type: compute
      tolerations:
        - key: "node-role.kubernetes.io/master"
          operator: "Exists"
          effect: "NoSchedule"
      affinity:
        nodeAffinity:
          requiredDuringSchedulingIgnoredDuringExecution:
            nodeSelectorTerms:
              - matchExpressions:
                  - key: "node-type"
                    operator: "In"
                    values: ["compute"]
      containers:
        - name: sequencer-node
          image: sequencer:latest
          ports:
            - containerPort: 8080
              name: http
```

## Best Practices

1. **Replicas**: Use odd numbers for quorum-based systems
2. **Update Strategy**: Use RollingUpdate for zero-downtime deployments
3. **Security**: Always run as non-root user
4. **Affinity**: Use pod anti-affinity for high availability
5. **Resources**: Set appropriate resource requests and limits
6. **Monitoring**: Add monitoring annotations for observability
7. **Graceful Shutdown**: Set appropriate termination grace period
