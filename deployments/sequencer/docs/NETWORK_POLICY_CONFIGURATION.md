# NetworkPolicy Configuration Guide

This document describes all available configuration options for the NetworkPolicy construct, which controls network traffic between pods in Kubernetes.

## Overview

NetworkPolicy is a Kubernetes resource that allows you to control traffic flow at the network level. It enables:
- **Ingress rules**: Control which pods/services can send traffic to your pods
- **Egress rules**: Control which pods/services your pods can send traffic to
- **Isolation**: Enforce network segmentation and security boundaries

## Basic Configuration

```yaml
networkPolicy:
  enabled: true
  name: "sequencer-node-networkpolicy"
  annotations: {}
  labels: {}
  podSelector:
    matchLabels:
      app: sequencer
      service: node
  policyTypes: ["Ingress", "Egress"]
  ingress: []
  egress: []
```

## Simple Ingress Rule

Allow traffic from specific pods:

```yaml
networkPolicy:
  enabled: true
  podSelector:
    matchLabels:
      app: sequencer
      service: node
  ingress:
    - from:
        - podSelector:
            matchLabels:
              app: frontend
```

## Allow Traffic from Multiple Sources

```yaml
networkPolicy:
  enabled: true
  podSelector:
    matchLabels:
      app: sequencer
      service: node
  ingress:
    - from:
        # Allow from specific pods
        - podSelector:
            matchLabels:
              app: frontend
        # Allow from specific namespace
        - namespaceSelector:
            matchLabels:
              name: production
        # Allow from specific IP range
        - ipBlock:
            cidr: 10.0.0.0/8
            except:
              - 10.0.1.0/24
```

## Port-Specific Rules

Only allow traffic on specific ports:

```yaml
networkPolicy:
  enabled: true
  podSelector:
    matchLabels:
      app: sequencer
      service: node
  ingress:
    - ports:
        - protocol: TCP
          port: 8080
        - protocol: TCP
          port: 9090
      from:
        - podSelector:
            matchLabels:
              app: frontend
```

## Egress Rules

Control outbound traffic:

```yaml
networkPolicy:
  enabled: true
  podSelector:
    matchLabels:
      app: sequencer
      service: node
  egress:
    # Allow outbound to database pods
    - to:
        - podSelector:
            matchLabels:
              app: database
      ports:
        - protocol: TCP
          port: 5432
    # Allow outbound to external services
    - to:
        - ipBlock:
            cidr: 0.0.0.0/0
      ports:
        - protocol: TCP
          port: 443
        - protocol: TCP
          port: 80
    # Allow DNS
    - to:
        - namespaceSelector:
            matchLabels:
              name: kube-system
      ports:
        - protocol: UDP
          port: 53
```

## Advanced: Multiple Rules with Different Sources

```yaml
networkPolicy:
  enabled: true
  podSelector:
    matchLabels:
      app: sequencer
      service: node
  ingress:
    # Rule 1: Allow from frontend on port 8080
    - ports:
        - protocol: TCP
          port: 8080
      from:
        - podSelector:
            matchLabels:
              app: frontend
    # Rule 2: Allow from monitoring namespace on port 9090
    - ports:
        - protocol: TCP
          port: 9090
      from:
        - namespaceSelector:
            matchLabels:
              name: monitoring
    # Rule 3: Allow from specific IP range (all ports)
    - from:
        - ipBlock:
            cidr: 192.168.1.0/24
```

## Match Expressions (Advanced Selectors)

Use matchExpressions for more complex label matching:

```yaml
networkPolicy:
  enabled: true
  podSelector:
    matchLabels:
      app: sequencer
    matchExpressions:
      - key: tier
        operator: In
        values: ["backend", "middleware"]
  ingress:
    - from:
        - podSelector:
            matchExpressions:
              - key: environment
                operator: In
                values: ["production", "staging"]
```

## Default Behavior

- **`podSelector`**: If empty (no `matchLabels` and no `matchExpressions`), automatically defaults to pod labels. This ensures:
  - ✅ Selector always matches the pods the policy applies to
  - ✅ No manual synchronization needed when pod labels change
  - ✅ Prevents configuration drift between pod labels and NetworkPolicy selector
- **`policyTypes`**: If empty, auto-detected based on presence of `ingress` or `egress` rules
- **Ingress**: If no `ingress` rules are specified, all ingress traffic is denied (default deny)
- **Egress**: If no `egress` rules are specified, all egress traffic is denied (default deny)
- **Empty arrays**: If empty arrays are provided, all traffic of that type is denied

## Configuration Options

### `enabled` (boolean)
- **Default**: `false`
- **Description**: Whether to create the NetworkPolicy resource
- **Example**: `enabled: true` to enable NetworkPolicy creation

### `name` (string, optional)
- **Default**: `sequencer-{service_name}-networkpolicy`
- **Description**: Custom name for the NetworkPolicy resource
- **Example**: `name: "my-custom-network-policy"`

### `annotations` (dict)
- **Default**: `{}`
- **Description**: Kubernetes annotations to add to the NetworkPolicy
- **Example**:
  ```yaml
  annotations:
    description: "Network policy for sequencer node service"
  ```

### `labels` (dict)
- **Default**: `{}`
- **Description**: Additional labels to add to the NetworkPolicy (merged with common labels)
- **Example**:
  ```yaml
  labels:
    component: networking
  ```

### `podSelector` (dict)
- **Default**: Pod labels (if empty - auto-defaults to pod labels)
- **Description**: Selects which pods this policy applies to
- **Automatic Default Behavior**: If `podSelector` is empty (no `matchLabels` and no `matchExpressions`), the system automatically uses the pod labels. This ensures the selector stays in sync with pod labels and prevents configuration drift.
- **Override for Advanced Use Cases**: You can explicitly set `matchLabels` or `matchExpressions` to select different pods (e.g., selecting pods from multiple services).
- **Properties**:
  - `matchLabels` (dict): Label key-value pairs that must match (auto-defaults to pod labels if empty)
  - `matchExpressions` (list): Label selector requirements using operators (In, NotIn, Exists, DoesNotExist)
- **Example**:
  ```yaml
  podSelector:
    matchLabels:
      app: sequencer
      service: node
    matchExpressions:
      - key: tier
        operator: In
        values: ["backend"]
  ```

### `policyTypes` (list)
- **Default**: Auto-detected from presence of `ingress`/`egress` rules
- **Description**: List of policy types to apply ("Ingress", "Egress", or both)
- **Example**: `policyTypes: ["Ingress", "Egress"]`

### `ingress` (list)
- **Default**: `[]` (deny all ingress)
- **Description**: List of ingress rules specifying allowed incoming traffic
- **Properties**:
  - `ports` (list): List of allowed ports
    - `protocol` (string): "TCP", "UDP", or "SCTP"
    - `port` (int or string): Port number or named port
  - `from` (list): List of traffic sources
    - `podSelector` (dict): Select pods as source
    - `namespaceSelector` (dict): Select namespaces as source
    - `ipBlock` (dict): Select IP CIDR blocks
      - `cidr` (string): CIDR notation (e.g., "10.0.0.0/8")
      - `except` (list): CIDR blocks to exclude
- **Example**:
  ```yaml
  ingress:
    - ports:
        - protocol: TCP
          port: 8080
      from:
        - podSelector:
            matchLabels:
              app: frontend
        - namespaceSelector:
            matchLabels:
              name: production
        - ipBlock:
            cidr: 10.0.0.0/8
            except:
              - 10.0.1.0/24
  ```

### `egress` (list)
- **Default**: `[]` (deny all egress)
- **Description**: List of egress rules specifying allowed outgoing traffic
- **Properties**:
  - `ports` (list): List of allowed ports (same format as ingress)
  - `to` (list): List of traffic destinations (same format as `from` in ingress)
- **Example**:
  ```yaml
  egress:
    - ports:
        - protocol: TCP
          port: 5432
      to:
        - podSelector:
            matchLabels:
              app: database
    - ports:
        - protocol: TCP
          port: 443
      to:
        - ipBlock:
            cidr: 0.0.0.0/0
  ```

## Common Use Cases

### 1. Isolate Services
```yaml
networkPolicy:
  enabled: true
  podSelector:
    matchLabels:
      app: sequencer
  # No ingress/egress = completely isolated
  ingress: []
  egress: []
```

### 2. Allow Only Internal Traffic
```yaml
networkPolicy:
  enabled: true
  podSelector:
    matchLabels:
      app: sequencer
  ingress:
    - from:
        - namespaceSelector:
            matchLabels:
              name: internal
  egress:
    - to:
        - namespaceSelector:
            matchLabels:
              name: internal
```

### 3. Allow Internet Access
```yaml
networkPolicy:
  enabled: true
  podSelector:
    matchLabels:
      app: sequencer
  egress:
    - to:
        - ipBlock:
            cidr: 0.0.0.0/0
      ports:
        - protocol: TCP
          port: 443
        - protocol: TCP
          port: 80
        - protocol: UDP
          port: 53  # DNS
```

### 4. Multi-Tier Application
```yaml
networkPolicy:
  enabled: true
  podSelector:
    matchLabels:
      app: sequencer
      tier: backend
  ingress:
    - from:
        - podSelector:
            matchLabels:
              tier: frontend
      ports:
        - protocol: TCP
          port: 8080
  egress:
    - to:
        - podSelector:
            matchLabels:
              tier: database
      ports:
        - protocol: TCP
          port: 5432
```

## Important Notes

1. **Network Policy Enforcement**: NetworkPolicy only works if your cluster has a CNI plugin that supports NetworkPolicy (e.g., Calico, Weave, Cilium)

2. **Default Behavior**: 
   - If no NetworkPolicy applies to a pod, all traffic is allowed (default allow)
   - Once a NetworkPolicy applies, the default becomes deny-all for that policy type

3. **Multiple Policies**: Multiple NetworkPolicies can apply to the same pod. The union of all rules is applied (OR logic)

4. **Namespace Scope**: NetworkPolicy is namespaced - each policy only applies within its namespace

5. **Label Selectors**: Use the same labels that are applied to your pods/namespaces for consistency

## Troubleshooting

- **No connectivity**: Verify that your CNI plugin supports NetworkPolicy
- **Too restrictive**: Check if your `ingress`/`egress` rules are correct
- **DNS issues**: Ensure you allow UDP port 53 to kube-system namespace for DNS
- **Service discovery**: Remember that NetworkPolicy applies to pods, not Services

