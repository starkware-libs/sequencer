# PriorityClass Configuration Guide

This document describes all available configuration options for the PriorityClass construct, which controls pod scheduling priority and preemption behavior in Kubernetes.

## Overview

PriorityClass is a Kubernetes resource that allows you to define different priority levels for pods. Higher priority pods can preempt lower priority pods when resources are scarce, ensuring that critical workloads get the resources they need.

**Important**: PriorityClass is a **cluster-scoped** resource (not namespaced), meaning there can only be one PriorityClass with a given name in the entire cluster.

## Basic Configuration

```yaml
priorityClass:
  enabled: true
  name: "sequencer-high-priority"
  value: 1000
  globalDefault: false
  description: "High priority for sequencer workloads"
  preemptionPolicy: "PreemptLowerPriority"
```

## Simple Configuration

```yaml
priorityClass:
  enabled: true
  value: 1000
```

This creates a PriorityClass named `sequencer-{service_name}-priorityclass` with value 1000.

## High Priority Workload

```yaml
priorityClass:
  enabled: true
  name: "sequencer-production"
  value: 2000
  description: "High priority for production sequencer workloads"
  preemptionPolicy: "PreemptLowerPriority"
```

## Default PriorityClass

Mark this PriorityClass as the default for all pods that don't specify a priorityClassName:

```yaml
priorityClass:
  enabled: true
  name: "sequencer-default"
  value: 1000
  globalDefault: true
  description: "Default priority class for sequencer pods"
```

**Important**: Only one PriorityClass in the cluster can have `globalDefault: true`.

## No Preemption Policy

Prevent pods with this PriorityClass from preempting other pods:

```yaml
priorityClass:
  enabled: true
  name: "sequencer-non-preempting"
  value: 1500
  preemptionPolicy: "Never"
  description: "High priority but won't preempt other pods"
```

## Configuration Options

### `enabled` (boolean)
- **Default**: `false`
- **Description**: Whether to create the PriorityClass resource
- **Example**: `enabled: true` to enable PriorityClass creation

### `name` (string, optional)
- **Default**: `sequencer-{service_name}-priorityclass`
- **Description**: Custom name for the PriorityClass resource
- **Example**: `name: "sequencer-high-priority"`
- **Note**: PriorityClass names must be unique cluster-wide

### `annotations` (dict)
- **Default**: `{}`
- **Description**: Kubernetes annotations to add to the PriorityClass
- **Example**:
  ```yaml
  annotations:
    description: "Priority class for critical sequencer workloads"
  ```

### `labels` (dict)
- **Default**: `{}`
- **Description**: Additional labels to add to the PriorityClass (merged with common labels)
- **Example**:
  ```yaml
  labels:
    component: scheduling
  ```

### `value` (integer)
- **Required**: Yes
- **Description**: Priority value (higher = more important)
- **Range**: Typically 0-1000000000, but can be any integer
- **Example**: `value: 1000` for normal priority, `value: 2000` for high priority
- **Note**: Higher values indicate higher priority

### `globalDefault` (boolean)
- **Default**: `false`
- **Description**: Whether this PriorityClass should be used as the default for pods that don't specify a `priorityClassName`
- **Example**: `globalDefault: true`
- **Important**: Only one PriorityClass in the cluster can have `globalDefault: true`

### `description` (string, optional)
- **Default**: `null`
- **Description**: Human-readable description of the PriorityClass
- **Example**: `description: "High priority for production workloads"`

### `preemptionPolicy` (string, optional)
- **Default**: `null` (uses cluster default, typically "PreemptLowerPriority")
- **Description**: Policy for pod preemption
- **Values**:
  - `"PreemptLowerPriority"`: Pods with this PriorityClass can preempt pods with lower priority
  - `"Never"`: Pods with this PriorityClass will never preempt other pods
- **Example**: `preemptionPolicy: "Never"`

## Using PriorityClass in Pods

After creating a PriorityClass, reference it in your pod configuration using `priorityClassName`:

```yaml
priorityClassName: "sequencer-high-priority"
```

The PriorityClass must be created before or at the same time as the pods that reference it.

## Common Use Cases

### 1. Production vs Development

```yaml
# High priority for production
priorityClass:
  enabled: true
  name: "sequencer-production"
  value: 2000
  description: "Production workloads - highest priority"

# Lower priority for development
priorityClass:
  enabled: true
  name: "sequencer-development"
  value: 500
  description: "Development workloads - lower priority"
```

### 2. Critical System Components

```yaml
priorityClass:
  enabled: true
  name: "sequencer-system-critical"
  value: 1000000
  description: "Critical system components that must run"
  preemptionPolicy: "PreemptLowerPriority"
```

### 3. Background Jobs

```yaml
priorityClass:
  enabled: true
  name: "sequencer-background"
  value: 100
  description: "Background jobs - lowest priority"
  preemptionPolicy: "Never"
```

### 4. Default PriorityClass

Set up a default PriorityClass for all pods in the cluster:

```yaml
priorityClass:
  enabled: true
  name: "sequencer-default"
  value: 1000
  globalDefault: true
  description: "Default priority class for all sequencer pods"
```

Then in your pod configuration, you can omit `priorityClassName` and it will automatically use this default.

## Priority Value Recommendations

While there are no strict rules for priority values, here are common ranges:

- **0-100**: Background/batch jobs, low priority
- **100-1000**: Development/testing workloads
- **1000-5000**: Normal production workloads
- **5000-10000**: High priority production workloads
- **10000+**: Critical system components

## Important Notes

1. **Cluster-Scoped**: PriorityClass is a cluster-scoped resource. Only one PriorityClass with a given name can exist in the cluster.

2. **Global Default**: Only one PriorityClass in the cluster can have `globalDefault: true`. If multiple PriorityClasses have this set, Kubernetes will reject them.

3. **Preemption**: Preemption requires the cluster to have the `PodPriority` feature enabled (enabled by default in Kubernetes 1.11+).

4. **Priority Values**: Higher values indicate higher priority. There's no maximum value, but values above 1,000,000,000 (1 billion) are typically reserved for system components.

5. **Preemption Policy**: 
   - `"PreemptLowerPriority"` (default): Pods can preempt lower priority pods
   - `"Never"`: Pods will wait for resources rather than preempting

6. **Resource Limits**: Preemption doesn't bypass resource limits. A pod can only preempt pods if it can fit on the node after preemption.

7. **Namespace Isolation**: Even with priority classes, pods in different namespaces still respect resource quotas.

## Troubleshooting

- **PriorityClass not found**: Ensure the PriorityClass is created before pods that reference it
- **Preemption not working**: Verify that `PodPriority` feature is enabled in your cluster
- **Multiple global defaults**: Only one PriorityClass can have `globalDefault: true` cluster-wide
- **Pod stuck pending**: Check if higher priority pods are consuming all resources and consider adjusting priority values

## Integration with Pod Configuration

Once a PriorityClass is created, you reference it in your pod configuration:

```yaml
# In ServiceConfig
priorityClassName: "sequencer-high-priority"
```

The PriorityClass will automatically be used when creating the pod spec in `PodBuilder`.

