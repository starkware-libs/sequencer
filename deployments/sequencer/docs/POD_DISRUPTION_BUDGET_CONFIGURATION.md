# PodDisruptionBudget Configuration Guide

This document describes all available configuration options for the PodDisruptionBudget construct.

## Basic Configuration

```yaml
podDisruptionBudget:
  enabled: true
  name: ""  # Optional: defaults to sequencer-{service}-pdb
  annotations: {}
  labels: {}
  selector:
    matchLabels: {}  # Uses service labels if empty
    matchExpressions: []
  minAvailable: 1  # OR use maxUnavailable (not both)
  maxUnavailable: null  # OR use minAvailable (not both)
  unhealthyPodEvictionPolicy: "IfHealthyBudget"  # IfHealthyBudget, AlwaysAllow
```

## Advanced Configuration

```yaml
podDisruptionBudget:
  enabled: true
  name: "sequencer-custom-pdb"
  annotations:
    "custom.annotation/key": "value"
  labels:
    "custom.label/key": "value"
  selector:
    matchLabels:
      app: sequencer
      service: node
    matchExpressions:
      - key: environment
        operator: In
        values: ["production"]
  maxUnavailable: "25%"  # Percentage of pods that can be unavailable
  unhealthyPodEvictionPolicy: "AlwaysAllow"
```

## Configuration Options

### Top-Level Configuration

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `enabled` | `bool` | Yes | `false` | Enable PodDisruptionBudget resource creation |
| `name` | `string` | No | `sequencer-{service}-pdb` | Custom name for the PodDisruptionBudget resource |
| `annotations` | `dict` | No | `{}` | Kubernetes annotations for the PodDisruptionBudget resource |
| `labels` | `dict` | No | `{}` | Kubernetes labels for the PodDisruptionBudget resource (merged with service labels) |
| `selector` | `dict` | No | Uses service labels | Pod selector configuration |

### Selector Configuration

The selector determines which pods the PDB applies to. **If the selector is empty or not specified, it automatically defaults to the pod labels**, ensuring the selector stays in sync with pod labels and preventing configuration drift.

```yaml
selector:
  matchLabels:
    app: sequencer
    service: node
  matchExpressions:
    - key: environment
      operator: In
      values: ["production", "staging"]
```

**Automatic Default Behavior**: If `selector` is empty (no `matchLabels` and no `matchExpressions`), the system automatically uses the pod labels. This ensures:
- ✅ Selector always matches the pods it's meant to protect
- ✅ No manual synchronization needed when pod labels change
- ✅ Prevents configuration drift between pod labels and PDB selector

**Override for Advanced Use Cases**: You can explicitly set `matchLabels` or `matchExpressions` to select different pods (e.g., selecting pods from multiple services).

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `matchLabels` | `dict` | No | Label selector using exact match (auto-defaults to pod labels if empty) |
| `matchExpressions` | `list` | No | Advanced label selector with operators (In, NotIn, Exists, DoesNotExist) |

### Availability Constraints

**Important**: Only one of `minAvailable` or `maxUnavailable` can be set at a time.

#### Using minAvailable

```yaml
podDisruptionBudget:
  minAvailable: 2  # At least 2 pods must be available
  # OR
  minAvailable: "50%"  # At least 50% of pods must be available
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `minAvailable` | `int\|string` | No* | Minimum number or percentage of pods that must be available (e.g., `2` or `"50%"`) |

#### Using maxUnavailable

```yaml
podDisruptionBudget:
  maxUnavailable: 1  # At most 1 pod can be unavailable
  # OR
  maxUnavailable: "25%"  # At most 25% of pods can be unavailable
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `maxUnavailable` | `int\|string` | No* | Maximum number or percentage of pods that can be unavailable (e.g., `1` or `"25%"`) |

\* Either `minAvailable` or `maxUnavailable` must be set, but not both.

### Unhealthy Pod Eviction Policy

```yaml
podDisruptionBudget:
  unhealthyPodEvictionPolicy: "IfHealthyBudget"  # or "AlwaysAllow"
```

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `unhealthyPodEvictionPolicy` | `string` | No | `IfHealthyBudget` | Policy for evicting unhealthy pods: `IfHealthyBudget` (only if budget allows), `AlwaysAllow` (always allow eviction) |

## Examples

### Basic PDB with Minimum Available Pods

```yaml
podDisruptionBudget:
  enabled: true
  minAvailable: 2  # At least 2 pods must always be available
```

This ensures that during voluntary disruptions (like node drains), at least 2 pods remain available.

### PDB with Maximum Unavailable Percentage

```yaml
podDisruptionBudget:
  enabled: true
  maxUnavailable: "25%"  # At most 25% of pods can be unavailable
```

This allows up to 25% of pods to be unavailable during disruptions. For a deployment with 10 replicas, this means up to 2 pods can be unavailable.

### PDB with Custom Selector

```yaml
podDisruptionBudget:
  enabled: true
  selector:
    matchLabels:
      app: sequencer
      service: node
      environment: production
  maxUnavailable: 1
```

### PDB with Advanced Selector Expressions

```yaml
podDisruptionBudget:
  enabled: true
  selector:
    matchLabels:
      app: sequencer
    matchExpressions:
      - key: environment
        operator: In
        values: ["production", "staging"]
      - key: version
        operator: NotIn
        values: ["deprecated"]
  minAvailable: "50%"
```

### PDB for High Availability

```yaml
podDisruptionBudget:
  enabled: true
  name: "sequencer-ha-pdb"
  minAvailable: "75%"  # At least 75% of pods must be available
  unhealthyPodEvictionPolicy: "IfHealthyBudget"
```

### PDB for Small Deployments

```yaml
podDisruptionBudget:
  enabled: true
  maxUnavailable: 1  # Only 1 pod can be unavailable at a time
```

For deployments with 2-3 replicas, this ensures most pods remain available.

## Best Practices

1. **Choose minAvailable or maxUnavailable**: Use `minAvailable` for deployments where you know the minimum needed for operation. Use `maxUnavailable` when you want to allow some disruption.

2. **Percentage vs Absolute**: Use percentages for deployments that scale (HPA). Use absolute numbers for fixed-size deployments.

3. **Selector Accuracy**: Ensure the selector matches exactly the pods you want to protect. It should match the same labels used by your Deployment or StatefulSet.

4. **Multiple PDBs**: You can have multiple PDBs for different sets of pods in the same namespace.

5. **Unhealthy Pod Policy**: 
   - `IfHealthyBudget` (default): Only evict unhealthy pods if the budget allows it. More conservative.
   - `AlwaysAllow`: Always allow eviction of unhealthy pods. Use this if you want to prioritize removing unhealthy pods.

6. **Coordination with HPA**: When using HPA, consider using percentages rather than absolute numbers for better scalability.

7. **Testing**: Test your PDB configuration during planned maintenance to ensure it behaves as expected.

## Common Use Cases

### Protection During Rolling Updates

```yaml
podDisruptionBudget:
  enabled: true
  maxUnavailable: 1  # Only 1 pod can be disrupted during updates
```

### Protection for Stateful Workloads

```yaml
podDisruptionBudget:
  enabled: true
  minAvailable: 1  # At least 1 pod must remain available
```

### Protection for Stateless Services

```yaml
podDisruptionBudget:
  enabled: true
  maxUnavailable: "25%"  # Allow up to 25% disruption
```

## Generated Kubernetes Resource

The configuration generates a PodDisruptionBudget resource:

```yaml
apiVersion: policy/v1
kind: PodDisruptionBudget
metadata:
  name: sequencer-node-pdb
  namespace: default
  labels:
    app: sequencer
    service: sequencer-node
spec:
  selector:
    matchLabels:
      app: sequencer
      service: sequencer-node
  maxUnavailable: 50%
  unhealthyPodEvictionPolicy: IfHealthyBudget
```

## References

- [Kubernetes PodDisruptionBudget Documentation](https://kubernetes.io/docs/tasks/run-application/configure-pdb/)
- [PDB Best Practices](https://kubernetes.io/docs/tasks/run-application/configure-pdb/#specifying-a-poddisruptionbudget)

