# ServiceAccount Configuration Guide

This document describes all available configuration options for the ServiceAccount construct.

## Basic Configuration

```yaml
serviceAccount:
  enabled: true
  name: ""  # Optional: custom name, defaults to {service-name}-sa
  annotations: {}
  labels: {}
```

## Advanced Configuration

```yaml
serviceAccount:
  enabled: true
  name: "my-custom-sa"
  annotations:
    "iam.gke.io/gcp-service-account": "my-service@project.iam.gserviceaccount.com"
    "eks.amazonaws.com/role-arn": "arn:aws:iam::123456789012:role/my-role"
    "azure.workload.identity/client-id": "12345678-1234-1234-1234-123456789012"
  labels:
    "app.kubernetes.io/component": "service-account"
    "app.kubernetes.io/part-of": "sequencer"
  automountServiceAccountToken: true
  imagePullSecrets: 
    - "my-registry-secret"
    - "gcr-secret"
    - "docker-hub-secret"
  secrets:
    - name: "my-secret"
      namespace: "default"
    - name: "another-secret"
      namespace: "kube-system"
```

## Configuration Options

### `enabled` (boolean)
- **Default**: `true`
- **Description**: Whether to create the ServiceAccount resource
- **Example**: `enabled: false` to disable ServiceAccount creation

### `name` (string, optional)
- **Default**: `{service-name}-sa`
- **Description**: Custom name for the ServiceAccount
- **Example**: `name: "my-custom-sa"`

### `annotations` (object)
- **Default**: `{}`
- **Description**: Annotations to add to the ServiceAccount metadata
- **Common Use Cases**:
  - **GKE Workload Identity**: `"iam.gke.io/gcp-service-account": "my-service@project.iam.gserviceaccount.com"`
  - **EKS IAM Roles**: `"eks.amazonaws.com/role-arn": "arn:aws:iam::123456789012:role/my-role"`
  - **Azure Workload Identity**: `"azure.workload.identity/client-id": "12345678-1234-1234-1234-123456789012"`

### `labels` (object)
- **Default**: `{}`
- **Description**: Labels to add to the ServiceAccount metadata
- **Example**:
  ```yaml
  labels:
    "app.kubernetes.io/component": "service-account"
    "app.kubernetes.io/part-of": "sequencer"
    "environment": "production"
  ```

### `automountServiceAccountToken` (boolean, optional)
- **Default**: `true`
- **Description**: Whether to automatically mount the service account token
- **Security Note**: Set to `false` for enhanced security if not needed

### `imagePullSecrets` (array of strings)
- **Default**: `[]`
- **Description**: List of image pull secret names to attach to the ServiceAccount
- **Example**:
  ```yaml
  imagePullSecrets:
    - "my-registry-secret"
    - "gcr-secret"
    - "docker-hub-secret"
  ```

### `secrets` (array of objects)
- **Default**: `[]`
- **Description**: List of secret references to attach to the ServiceAccount
- **Example**:
  ```yaml
  secrets:
    - name: "my-secret"
      namespace: "default"
    - name: "another-secret"
      namespace: "kube-system"
  ```

## Cloud Provider Integration Examples

### Google Kubernetes Engine (GKE) with Workload Identity

```yaml
serviceAccount:
  enabled: true
  name: "sequencer-gke-sa"
  annotations:
    "iam.gke.io/gcp-service-account": "sequencer@my-project.iam.gserviceaccount.com"
  labels:
    "app.kubernetes.io/component": "service-account"
  automountServiceAccountToken: true
  imagePullSecrets:
    - "gcr-secret"
```

### Amazon EKS with IAM Roles for Service Accounts (IRSA)

```yaml
serviceAccount:
  enabled: true
  name: "sequencer-eks-sa"
  annotations:
    "eks.amazonaws.com/role-arn": "arn:aws:iam::123456789012:role/sequencer-role"
  labels:
    "app.kubernetes.io/component": "service-account"
  automountServiceAccountToken: true
  imagePullSecrets:
    - "ecr-secret"
```

### Azure Kubernetes Service (AKS) with Workload Identity

```yaml
serviceAccount:
  enabled: true
  name: "sequencer-aks-sa"
  annotations:
    "azure.workload.identity/client-id": "12345678-1234-1234-1234-123456789012"
  labels:
    "app.kubernetes.io/component": "service-account"
  automountServiceAccountToken: true
  imagePullSecrets:
    - "acr-secret"
```

## Security Best Practices

1. **Minimal Permissions**: Only grant necessary permissions to the service account
2. **Token Management**: Set `automountServiceAccountToken: false` if not needed
3. **Secret Management**: Use external secret management systems when possible
4. **Labeling**: Use consistent labeling for better resource management
5. **Annotations**: Use cloud provider annotations for proper integration

## Generated Kubernetes Resource

The configuration above generates a ServiceAccount resource like this:

```yaml
apiVersion: v1
kind: ServiceAccount
metadata:
  name: my-custom-sa
  namespace: default
  annotations:
    iam.gke.io/gcp-service-account: my-service@project.iam.gserviceaccount.com
  labels:
    app: sequencer
    app.kubernetes.io/component: service-account
    service: sequencer-node
automountServiceAccountToken: true
imagePullSecrets:
  - name: my-registry-secret
secrets:
  - name: my-secret
    namespace: default
```
