# Deployment Configuration Guide

This document describes all available configuration options for the Deployment construct.

## Basic Configuration

```yaml
deployment:
  enabled: false
  replicas: 1
  strategy:
    type: "RollingUpdate"
  podAnnotations: {}
  serviceAccount:
    name: ""
    create: true
  securityContext: {}
  terminationGracePeriodSeconds: 300
```

## Advanced Configuration

```yaml
deployment:
  enabled: true
  replicas: 3
  strategy:
    type: "RollingUpdate"
    rollingUpdate:
      maxUnavailable: 1
      maxSurge: 1
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

## Configuration Options

### `enabled` (boolean)
- **Default**: `false`
- **Description**: Whether to create the Deployment resource
- **Example**: `enabled: true` to enable Deployment creation

### `replicas` (integer)
- **Default**: `1`
- **Description**: Number of replicas to maintain
- **Example**: `replicas: 3` for high availability

### `strategy` (object)
- **Default**: `{"type": "RollingUpdate"}`
- **Description**: Update strategy for the Deployment
- **Properties**:
  - `type` (string): Update strategy type ("RollingUpdate" or "Recreate")
  - `rollingUpdate` (object, optional): Rolling update configuration
    - `maxUnavailable` (integer): Maximum number of unavailable pods during update
    - `maxSurge` (integer): Maximum number of pods that can be created above desired count

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
deployment:
  enabled: true
  replicas: 3
  strategy:
    type: "RollingUpdate"
    rollingUpdate:
      maxUnavailable: 1
      maxSurge: 1
```

### Recreate Strategy

```yaml
deployment:
  enabled: true
  replicas: 3
  strategy:
    type: "Recreate"
```

### Blue-Green Deployment

```yaml
deployment:
  enabled: true
  replicas: 3
  strategy:
    type: "RollingUpdate"
    rollingUpdate:
      maxUnavailable: 0
      maxSurge: 3
```

## Security Configuration

### Non-Root User

```yaml
deployment:
  enabled: true
  securityContext:
    runAsUser: 1000
    runAsNonRoot: true
    runAsGroup: 1000
    fsGroup: 1000
```

### Pod Security Standards

```yaml
deployment:
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
deployment:
  enabled: true
  replicas: 3
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
deployment:
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
deployment:
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
deployment:
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

## Horizontal Pod Autoscaler Integration

### With HPA

```yaml
deployment:
  enabled: true
  replicas: 3  # Initial replicas, HPA will scale

hpa:
  enabled: true
  minReplicas: 3
  maxReplicas: 10
  targetCPUUtilizationPercentage: 70
  targetMemoryUtilizationPercentage: 80
```

### Without HPA

```yaml
deployment:
  enabled: true
  replicas: 5  # Fixed replica count

hpa:
  enabled: false
```

## Generated Kubernetes Resource

The configuration above generates a Deployment resource like this:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: sequencer-node-deployment
  namespace: default
spec:
  replicas: 3
  strategy:
    type: RollingUpdate
    rollingUpdate:
      maxUnavailable: 1
      maxSurge: 1
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
      containers:
        - name: sequencer-node
          image: sequencer:latest
          ports:
            - containerPort: 8080
              name: http
```

## Best Practices

1. **Replicas**: Use appropriate replica count for your workload
2. **Update Strategy**: Use RollingUpdate for zero-downtime deployments
3. **Security**: Always run as non-root user
4. **Affinity**: Use pod anti-affinity for high availability
5. **Resources**: Set appropriate resource requests and limits
6. **Monitoring**: Add monitoring annotations for observability
7. **HPA**: Use HPA for dynamic scaling based on metrics
8. **Graceful Shutdown**: Set appropriate termination grace period
