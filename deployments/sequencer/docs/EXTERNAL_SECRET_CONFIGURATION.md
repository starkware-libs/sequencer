# ExternalSecret Configuration Guide

This document describes all available configuration options for the ExternalSecret construct.

## Basic Configuration

```yaml
externalSecret:
  enabled: false
  name: ""  # Optional: defaults to {service-name}-external-secret
  secretStore:
    name: "my-secret-store"
    kind: "SecretStore"  # or "ClusterSecretStore"
  data:
    - secretKey: "username"
      remoteKey: "my-secret/username"
      property: ""  # Optional: for nested properties
  targetName: ""  # Optional: defaults to {service-name}-secret
  template: {}  # Optional: secret template
  metadata: {}  # Optional: additional metadata
  deletionPolicy: "Retain"  # or "Delete"
```

## Advanced Configuration

```yaml
externalSecret:
  enabled: true
  name: "sequencer-external-secret"
  secretStore:
    name: "gcp-secret-store"
    kind: "ClusterSecretStore"
  data:
    - secretKey: "database-url"
      remoteKey: "sequencer/database-url"
      property: "value"
    - secretKey: "api-key"
      remoteKey: "sequencer/api-key"
      property: "data.api_key"
    - secretKey: "tls-cert"
      remoteKey: "sequencer/tls-cert"
      property: "data.certificate"
  targetName: "sequencer-secrets"
  template:
    type: "Opaque"
    metadata:
      labels:
        "app.kubernetes.io/component": "secrets"
  metadata:
    annotations:
      "external-secrets.io/refresh-interval": "1h"
  deletionPolicy: "Retain"
```

## Configuration Options

### `enabled` (boolean)
- **Default**: `false`
- **Description**: Whether to create the ExternalSecret resource
- **Example**: `enabled: true` to enable ExternalSecret creation

### `name` (string, optional)
- **Default**: `{service-name}-external-secret`
- **Description**: Custom name for the ExternalSecret resource
- **Example**: `name: "my-external-secret"`

### `secretStore` (object)
- **Required**: Yes
- **Description**: Reference to the SecretStore or ClusterSecretStore
- **Properties**:
  - `name` (string): Name of the secret store
  - `kind` (string): Either "SecretStore" or "ClusterSecretStore"

### `data` (array of objects)
- **Required**: Yes
- **Description**: List of secret data mappings
- **Properties**:
  - `secretKey` (string): Key name in the generated secret
  - `remoteKey` (string): Key name in the external secret store
  - `property` (string, optional): Nested property path in the remote secret

### `targetName` (string, optional)
- **Default**: `{service-name}-secret`
- **Description**: Name of the target secret to create
- **Example**: `targetName: "my-app-secrets"`

### `template` (object, optional)
- **Default**: `{}`
- **Description**: Template for the generated secret
- **Example**:
  ```yaml
  template:
    type: "Opaque"
    metadata:
      labels:
        "app.kubernetes.io/component": "secrets"
  ```

### `metadata` (object, optional)
- **Default**: `{}`
- **Description**: Additional metadata for the ExternalSecret resource
- **Example**:
  ```yaml
  metadata:
    annotations:
      "external-secrets.io/refresh-interval": "1h"
      "external-secrets.io/refresh-time": "2023-01-01T00:00:00Z"
  ```

### `deletionPolicy` (string)
- **Default**: `"Retain"`
- **Description**: What happens to the target secret when ExternalSecret is deleted
- **Values**: `"Retain"` or `"Delete"`

### `mountPath` (string, optional)
- **Default**: `"/etc/secrets"`
- **Description**: Path where the external secret will be mounted in the container
- **Note**: The secret is mounted as a single file `external-secret.json` at `{mountPath}/external-secret.json`
- **Example**:
  ```yaml
  externalSecret:
    enabled: true
    mountPath: "/custom/secrets/path"
  ```

## Provider-Specific Examples

### Google Cloud Secret Manager

```yaml
externalSecret:
  enabled: true
  name: "gcp-external-secret"
  secretStore:
    name: "gcp-secret-store"
    kind: "ClusterSecretStore"
  data:
    - secretKey: "database-password"
      remoteKey: "projects/my-project/secrets/database-password/versions/latest"
      property: "value"
    - secretKey: "api-key"
      remoteKey: "projects/my-project/secrets/api-key/versions/latest"
      property: "value"
  targetName: "gcp-secrets"
  deletionPolicy: "Retain"
```

### AWS Secrets Manager

```yaml
externalSecret:
  enabled: true
  name: "aws-external-secret"
  secretStore:
    name: "aws-secret-store"
    kind: "ClusterSecretStore"
  data:
    - secretKey: "database-url"
      remoteKey: "sequencer/database-url"
      property: "value"
    - secretKey: "api-key"
      remoteKey: "sequencer/api-key"
      property: "value"
  targetName: "aws-secrets"
  deletionPolicy: "Retain"
```

### Azure Key Vault

```yaml
externalSecret:
  enabled: true
  name: "azure-external-secret"
  secretStore:
    name: "azure-secret-store"
    kind: "ClusterSecretStore"
  data:
    - secretKey: "database-password"
      remoteKey: "database-password"
      property: "value"
    - secretKey: "api-key"
      remoteKey: "api-key"
      property: "value"
  targetName: "azure-secrets"
  deletionPolicy: "Retain"
```

### HashiCorp Vault

```yaml
externalSecret:
  enabled: true
  name: "vault-external-secret"
  secretStore:
    name: "vault-secret-store"
    kind: "ClusterSecretStore"
  data:
    - secretKey: "database-password"
      remoteKey: "secret/data/sequencer"
      property: "data.database_password"
    - secretKey: "api-key"
      remoteKey: "secret/data/sequencer"
      property: "data.api_key"
  targetName: "vault-secrets"
  deletionPolicy: "Retain"
```

## Secret Store Configuration

### ClusterSecretStore for GCP

```yaml
apiVersion: external-secrets.io/v1beta1
kind: ClusterSecretStore
metadata:
  name: gcp-secret-store
spec:
  provider:
    gcpsm:
      projectId: "my-project"
      auth:
        workloadIdentity:
          clusterLocation: "us-central1"
          clusterName: "my-cluster"
          serviceAccountRef:
            name: "external-secrets-sa"
            namespace: "external-secrets-system"
```

### ClusterSecretStore for AWS

```yaml
apiVersion: external-secrets.io/v1beta1
kind: ClusterSecretStore
metadata:
  name: aws-secret-store
spec:
  provider:
    aws:
      service: SecretsManager
      region: us-west-2
      auth:
        jwt:
          serviceAccountRef:
            name: external-secrets-sa
            namespace: external-secrets-system
```

## Generated Kubernetes Resource

The configuration above generates an ExternalSecret resource like this:

```yaml
apiVersion: external-secrets.io/v1beta1
kind: ExternalSecret
metadata:
  name: sequencer-external-secret
  namespace: default
  annotations:
    external-secrets.io/refresh-interval: 1h
spec:
  secretStoreRef:
    name: gcp-secret-store
    kind: ClusterSecretStore
  target:
    name: sequencer-secrets
    deletionPolicy: Retain
  data:
    - secretKey: database-url
      remoteRef:
        key: sequencer/database-url
        property: value
    - secretKey: api-key
      remoteRef:
        key: sequencer/api-key
        property: data.api_key
  template:
    type: Opaque
    metadata:
      labels:
        app.kubernetes.io/component: secrets
```

## Mounting in Pods

The ExternalSecret is **automatically mounted** in pods when `enabled: true`. The target secret is mounted as a **directory** at the specified mount path (default: `/etc/secrets`). **All secret keys become individual files** in this directory.

**Example**: If your ExternalSecret creates a secret with keys `{database-url: "...", api-key: "..."}`, they will be mounted as:
- `/etc/secrets/database-url`
- `/etc/secrets/api-key`

```yaml
externalSecret:
  enabled: true
  secretStore:
    name: "gcp-secret-store"
    kind: "ClusterSecretStore"
  data:
    - secretKey: database-url  # Any key name - becomes a file in the mount directory
      remoteKey: "sequencer/database-url"
      property: "value"
    - secretKey: api-key  # Another key - becomes another file
      remoteKey: "sequencer/api-key"
      property: "value"
  targetName: "sequencer-secrets"
  # mountPath: /etc/external-secrets  # Optional: Override default mount path (default: "/etc/secrets")
  # Note: Use a different path if Secret is also enabled to avoid conflicts
```

The generated volume mount looks like:

```yaml
volumeMounts:
  - name: sequencer-secrets-secrets-volume
    mountPath: /etc/secrets  # All ExternalSecret keys become files here
    readOnly: true
```

**Important**: If you also use Secret, make sure they use different mount paths to avoid conflicts. For example:
- Secret: `mountPath: /etc/secrets`
- ExternalSecret: `mountPath: /etc/external-secrets`

## Automatic Container Arguments

When an ExternalSecret is enabled, the container **automatically receives** the `--config_file` argument pointing to the mounted secret directory:

```yaml
args:
  - --config_file
  - /config/sequencer/presets/  # from ConfigMap (always present)
  - --config_file
  - /etc/secrets   # from ExternalSecret (if enabled) - directory containing all secret files
```

Your application should then read the specific files it needs from this directory (e.g., `/etc/secrets/database-url`, `/etc/secrets/api-key`).

## Best Practices

1. **Naming**: Use descriptive names for secret keys and remote keys
2. **Property Paths**: Use dot notation for nested properties (e.g., `data.api_key`)
3. **Deletion Policy**: Use "Retain" for production, "Delete" for development
4. **Refresh Intervals**: Set appropriate refresh intervals for secret rotation
5. **Labels**: Use consistent labeling for better resource management
6. **Security**: Ensure proper RBAC permissions for the External Secrets Operator
