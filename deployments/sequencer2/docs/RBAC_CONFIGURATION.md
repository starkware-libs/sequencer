# RBAC Configuration Guide

This document describes all available configuration options for RBAC (Role-Based Access Control) resources, including Role, RoleBinding, ClusterRole, and ClusterRoleBinding.

## Overview

RBAC in Kubernetes controls who can access what resources and what actions they can perform. You can choose between:
- **Role/RoleBinding**: Namespaced resources that apply only within a specific namespace
- **ClusterRole/ClusterRoleBinding**: Cluster-scoped resources that apply cluster-wide

## Basic Configuration

### Namespaced Role

```yaml
rbac:
  enabled: true
  type: Role
  rules:
    - apiGroups: [""]
      resources: ["pods"]
      verbs: ["get", "list", "watch"]
  subjects:
    - kind: ServiceAccount
      name: sequencer-node-sa
```

### Cluster-Scoped ClusterRole

```yaml
rbac:
  enabled: true
  type: ClusterRole
  rules:
    - apiGroups: [""]
      resources: ["nodes"]
      verbs: ["get", "list"]
  subjects:
    - kind: ServiceAccount
      name: sequencer-node-sa
      namespace: default
```

## Simple Role with ServiceAccount

```yaml
rbac:
  enabled: true
  type: Role
  rules:
    - apiGroups: [""]
      resources: ["pods", "configmaps"]
      verbs: ["get", "list", "watch"]
  # subjects will default to the service account if not specified
```

## Multiple Rules

```yaml
rbac:
  enabled: true
  type: Role
  rules:
    - apiGroups: [""]
      resources: ["pods", "services"]
      verbs: ["get", "list", "watch"]
    - apiGroups: ["apps"]
      resources: ["deployments"]
      verbs: ["get", "list", "watch", "create", "update", "patch"]
    - apiGroups: [""]
      resources: ["secrets"]
      resourceNames: ["my-secret"]
      verbs: ["get"]
```

## Multiple Subjects

```yaml
rbac:
  enabled: true
  type: Role
  rules:
    - apiGroups: [""]
      resources: ["pods"]
      verbs: ["get", "list"]
  subjects:
    - kind: ServiceAccount
      name: sequencer-node-sa
    - kind: User
      name: "system:serviceaccount:default:admin"
      apiGroup: rbac.authorization.k8s.io
    - kind: Group
      name: "system:authenticated"
      apiGroup: rbac.authorization.k8s.io
```

## ClusterRole with Non-Resource URLs

```yaml
rbac:
  enabled: true
  type: ClusterRole
  rules:
    - nonResourceURLs: ["/metrics", "/healthz"]
      verbs: ["get"]
  subjects:
    - kind: ServiceAccount
      name: monitoring-sa
      namespace: monitoring
```

## Advanced Configuration with Custom Names

```yaml
rbac:
  enabled: true
  type: Role
  roleName: "sequencer-custom-role"
  roleBindingName: "sequencer-custom-binding"
  annotations:
    description: "Custom role for sequencer service"
  labels:
    component: rbac
  rules:
    - apiGroups: [""]
      resources: ["pods"]
      verbs: ["*"]
  subjects:
    - kind: ServiceAccount
      name: sequencer-node-sa
```

## Custom RoleRef

```yaml
rbac:
  enabled: true
  type: Role
  rules:
    - apiGroups: [""]
      resources: ["pods"]
      verbs: ["get", "list"]
  roleRef:
    apiGroup: rbac.authorization.k8s.io
    kind: Role
    name: sequencer-custom-role
  subjects:
    - kind: ServiceAccount
      name: sequencer-node-sa
```

## Configuration Options

### `enabled` (boolean)
- **Default**: `false`
- **Description**: Whether to create RBAC resources
- **Example**: `enabled: true` to enable RBAC creation

### `type` (string)
- **Default**: `"Role"`
- **Description**: Type of RBAC resource to create
- **Values**: `"Role"` or `"ClusterRole"`
- **Example**: `type: "ClusterRole"` for cluster-scoped permissions

### `roleName` (string, optional)
- **Default**: `sequencer-{service_name}-role` or `sequencer-{service_name}-clusterrole`
- **Description**: Custom name for the Role/ClusterRole resource
- **Example**: `roleName: "sequencer-custom-role"`

### `roleBindingName` (string, optional)
- **Default**: `sequencer-{service_name}-rolebinding` or `sequencer-{service_name}-clusterrolebinding`
- **Description**: Custom name for the RoleBinding/ClusterRoleBinding resource
- **Example**: `roleBindingName: "sequencer-custom-binding"`

### `annotations` (dict)
- **Default**: `{}`
- **Description**: Kubernetes annotations to add to RBAC resources
- **Example**:
  ```yaml
  annotations:
    description: "Role for sequencer service"
  ```

### `labels` (dict)
- **Default**: `{}`
- **Description**: Additional labels to add to RBAC resources (merged with common labels)
- **Example**:
  ```yaml
  labels:
    component: rbac
  ```

### `rules` (list)
- **Required**: Yes (if enabled)
- **Description**: List of PolicyRule objects defining what resources and verbs are allowed
- **Properties**:
  - `apiGroups` (list): List of API groups (use `[""]` for core API group)
  - `resources` (list): List of resource types (e.g., `["pods", "services"]`)
  - `verbs` (list): List of allowed verbs (e.g., `["get", "list", "watch"]`)
  - `resourceNames` (list, optional): Specific resource names to restrict access to
  - `nonResourceURLs` (list, optional): Non-resource URLs (for ClusterRole only)
- **Example**:
  ```yaml
  rules:
    - apiGroups: [""]
      resources: ["pods"]
      verbs: ["get", "list", "watch"]
    - apiGroups: ["apps"]
      resources: ["deployments"]
      verbs: ["*"]
  ```

### `subjects` (list)
- **Required**: Yes (if enabled)
- **Description**: List of Subject objects defining who the RoleBinding applies to
- **Properties**:
  - `kind` (string): Kind of subject - `"ServiceAccount"`, `"User"`, or `"Group"`
  - `name` (string): Name of the subject
  - `namespace` (string, optional): Namespace for ServiceAccount (required for ServiceAccount)
  - `apiGroup` (string, optional): API group (usually `rbac.authorization.k8s.io` for User/Group)
- **Auto-defaults**: If not specified, defaults to the service account
- **Example**:
  ```yaml
  subjects:
    - kind: ServiceAccount
      name: sequencer-node-sa
      namespace: default
    - kind: User
      name: "system:serviceaccount:default:admin"
      apiGroup: rbac.authorization.k8s.io
  ```

### `roleRef` (dict, optional)
- **Default**: Auto-generated based on `roleName`
- **Description**: Custom RoleRef object for the RoleBinding
- **Properties**:
  - `apiGroup` (string): API group (usually `rbac.authorization.k8s.io`)
  - `kind` (string): Kind - `"Role"` or `"ClusterRole"`
  - `name` (string): Name of the Role/ClusterRole
- **Example**:
  ```yaml
  roleRef:
    apiGroup: rbac.authorization.k8s.io
    kind: Role
    name: sequencer-custom-role
  ```

## Common Use Cases

### 1. Read-Only Access to Pods

```yaml
rbac:
  enabled: true
  type: Role
  rules:
    - apiGroups: [""]
      resources: ["pods"]
      verbs: ["get", "list", "watch"]
```

### 2. Full Access to ConfigMaps and Secrets

```yaml
rbac:
  enabled: true
  type: Role
  rules:
    - apiGroups: [""]
      resources: ["configmaps", "secrets"]
      verbs: ["*"]
```

### 3. Deployment Management

```yaml
rbac:
  enabled: true
  type: Role
  rules:
    - apiGroups: ["apps"]
      resources: ["deployments"]
      verbs: ["get", "list", "watch", "create", "update", "patch", "delete"]
```

### 4. Cluster-Wide Node Access

```yaml
rbac:
  enabled: true
  type: ClusterRole
  rules:
    - apiGroups: [""]
      resources: ["nodes"]
      verbs: ["get", "list", "watch"]
  subjects:
    - kind: ServiceAccount
      name: sequencer-node-sa
      namespace: default
```

### 5. Access to Specific Secrets

```yaml
rbac:
  enabled: true
  type: Role
  rules:
    - apiGroups: [""]
      resources: ["secrets"]
      resourceNames: ["my-secret", "another-secret"]
      verbs: ["get"]
```

### 6. Custom Resource Access

```yaml
rbac:
  enabled: true
  type: ClusterRole
  rules:
    - apiGroups: ["custom.example.com"]
      resources: ["customresources"]
      verbs: ["get", "list", "watch"]
```

## Important Notes

1. **Role vs ClusterRole**: 
   - **Role**: Namespaced, applies only within the namespace
   - **ClusterRole**: Cluster-scoped, applies cluster-wide

2. **RoleBinding vs ClusterRoleBinding**:
   - **RoleBinding**: Binds subjects to a Role (namespaced)
   - **ClusterRoleBinding**: Binds subjects to a ClusterRole (cluster-scoped)
   - **Note**: You can bind a ClusterRole to a RoleBinding (grants permissions in that namespace only)

3. **ServiceAccount Namespace**: When using ServiceAccount as a subject, the namespace is required and defaults to the chart's namespace

4. **Verbs**: Common verbs include:
   - `get`, `list`, `watch` - Read operations
   - `create`, `update`, `patch` - Write operations
   - `delete` - Delete operations
   - `*` - All verbs

5. **API Groups**:
   - `[""]` - Core API group (pods, services, configmaps, secrets, etc.)
   - `["apps"]` - Apps API group (deployments, statefulsets, etc.)
   - `["rbac.authorization.k8s.io"]` - RBAC API group
   - Custom API groups for CRDs

6. **Non-Resource URLs**: Only available for ClusterRole, allows access to non-resource endpoints like `/metrics`, `/healthz`

7. **Auto-Defaults**: 
   - If `subjects` are not specified, defaults to the service account
   - If `roleRef` is not specified, auto-generates based on `roleName`
   - If `roleName` is not specified, auto-generates based on service name

## Troubleshooting

- **Permission denied**: Verify the rules include the necessary resources and verbs
- **Subject not found**: Ensure ServiceAccount exists before creating RoleBinding
- **Namespace required**: Remember that ServiceAccount subjects require a namespace
- **ClusterRole with RoleBinding**: You can use a ClusterRole with a RoleBinding to grant cluster-wide permissions within a single namespace

