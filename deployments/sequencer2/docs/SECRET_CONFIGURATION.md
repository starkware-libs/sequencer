# Secret Configuration Guide

This document describes all available configuration options for the Secret construct.

## Basic Configuration

```yaml
secret:
  enabled: false
  name: ""
  type: Opaque
  data: {}
  stringData: {}
  annotations: {}
  labels: {}
  immutable: false
```

## Advanced Configuration

```yaml
secret:
  enabled: true
  name: "sequencer-secrets"
  type: Opaque
  data:
    tls.crt: LS0tLS1CRUdJTi...  # Base64 encoded certificate
    tls.key: LS0tLS1CRUdJTi...  # Base64 encoded private key
  stringData:
    database-url: "postgresql://user:password@localhost:5432/db"
    api-key: "your-api-key-here"
    config.json: |
      {
        "database": {
          "host": "localhost",
          "port": 5432
        }
      }
  annotations:
    "backup.kubernetes.io/enabled": "true"
  labels:
    "app.kubernetes.io/component": "secrets"
  immutable: true
```

## Configuration Options

### `enabled` (boolean)
- **Default**: `false`
- **Description**: Whether to create the Secret resource
- **Example**: `enabled: true` to enable Secret creation

### `name` (string, optional)
- **Default**: `{service-name}-secret`
- **Description**: Custom name for the Secret resource
- **Example**: `name: "my-app-secrets"`

### `type` (string)
- **Default**: `"Opaque"`
- **Description**: Type of the Secret
- **Values**: `"Opaque"`, `"kubernetes.io/tls"`, `"kubernetes.io/dockerconfigjson"`, `"kubernetes.io/basic-auth"`, `"kubernetes.io/ssh-auth"`
- **Example**: `type: "kubernetes.io/tls"` for TLS certificates

### `data` (object)
- **Default**: `{}`
- **Description**: Base64-encoded secret data
- **Example**:
  ```yaml
  data:
    username: YWRtaW4=  # base64 encoded "admin"
    password: cGFzc3dvcmQ=  # base64 encoded "password"
  ```

### `stringData` (object)
- **Default**: `{}`
- **Description**: Plain text secret data (automatically base64 encoded)
- **Example**:
  ```yaml
  stringData:
    database-url: "postgresql://user:password@localhost:5432/db"
    api-key: "your-api-key-here"
  ```

### `annotations` (object)
- **Default**: `{}`
- **Description**: Annotations to add to the Secret metadata
- **Example**:
  ```yaml
  annotations:
    "backup.kubernetes.io/enabled": "true"
    "sealed-secrets.bitnami.com/encrypted": "true"
  ```

### `labels` (object)
- **Default**: `{}`
- **Description**: Labels to add to the Secret metadata
- **Example**:
  ```yaml
  labels:
    "app.kubernetes.io/component": "secrets"
    "environment": "production"
  ```

### `immutable` (boolean, optional)
- **Default**: `false`
- **Description**: Whether the Secret is immutable
- **Security Note**: Set to `true` for enhanced security

### `mountPath` (string, optional)
- **Default**: `"/etc/secrets"`
- **Description**: Path where the secret will be mounted in the container as a directory
- **Note**: All secret keys become individual files in this directory. If you also use ExternalSecret, use different mount paths to avoid conflicts.
- **Example**:
  ```yaml
  secret:
    enabled: true
    mountPath: "/custom/secrets/path"
  ```

## Secret Type Examples

### Opaque Secret (Default)

```yaml
secret:
  enabled: true
  name: "app-secrets"
  type: Opaque
  stringData:
    database-url: "postgresql://user:password@localhost:5432/db"
    api-key: "your-api-key-here"
    config.json: |
      {
        "database": {
          "host": "localhost",
          "port": 5432
        }
      }
```

### TLS Secret

```yaml
secret:
  enabled: true
  name: "tls-secret"
  type: kubernetes.io/tls
  data:
    tls.crt: LS0tLS1CRUdJTi...  # Base64 encoded certificate
    tls.key: LS0tLS1CRUdJTi...  # Base64 encoded private key
```

### Docker Registry Secret

```yaml
secret:
  enabled: true
  name: "docker-registry-secret"
  type: kubernetes.io/dockerconfigjson
  data:
    .dockerconfigjson: eyJhdXRocyI6eyJodHRwczovL2luZGV4LmRvY2tlci5pby92MS8iOnsidXNlcm5hbWUiOiJteS11c2VyIiwicGFzc3dvcmQiOiJteS1wYXNzd29yZCIsImF1dGgiOiJZV1J0YVc0Nk1UUTNNVEV5TkRVM01UUT0ifX19
```

### Basic Auth Secret

```yaml
secret:
  enabled: true
  name: "basic-auth-secret"
  type: kubernetes.io/basic-auth
  stringData:
    username: admin
    password: secretpassword
```

### SSH Secret

```yaml
secret:
  enabled: true
  name: "ssh-secret"
  type: kubernetes.io/ssh-auth
  data:
    ssh-privatekey: LS0tLS1CRUdJTi...  # Base64 encoded SSH private key
```

## Data vs StringData

### Using `data` (Base64 Encoded)

```yaml
secret:
  enabled: true
  name: "encoded-secret"
  type: Opaque
  data:
    username: YWRtaW4=  # "admin" base64 encoded
    password: cGFzc3dvcmQ=  # "password" base64 encoded
    binary-file: UklGRjIAAAEAAAAA...  # Binary file base64 encoded
```

### Using `stringData` (Plain Text)

```yaml
secret:
  enabled: true
  name: "plain-secret"
  type: Opaque
  stringData:
    username: admin
    password: password
    config.yaml: |
      database:
        host: localhost
        port: 5432
        username: admin
        password: secret
```

## Security Best Practices

### Immutable Secrets

```yaml
secret:
  enabled: true
  name: "immutable-secret"
  type: Opaque
  stringData:
    api-key: "your-secure-api-key"
  immutable: true
```

### Encrypted Secrets with Sealed Secrets

```yaml
secret:
  enabled: true
  name: "sealed-secret"
  type: Opaque
  annotations:
    "sealed-secrets.bitnami.com/encrypted": "AgBy3i4OJSWK+PiTySYZZA9rO43cGDEQAx..."
  data:
    secret-key: "encrypted-value"
```

### Backup-Enabled Secrets

```yaml
secret:
  enabled: true
  name: "backup-secret"
  type: Opaque
  annotations:
    "backup.kubernetes.io/enabled": "true"
    "backup.kubernetes.io/schedule": "0 2 * * *"
  labels:
    "backup.kubernetes.io/backup": "true"
  stringData:
    important-data: "backup-this"
```

## Generated Kubernetes Resource

The configuration above generates a Secret resource like this:

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: sequencer-secrets
  namespace: default
  labels:
    app: sequencer
    service: sequencer-node
    app.kubernetes.io/component: secrets
  annotations:
    backup.kubernetes.io/enabled: "true"
type: Opaque
data:
  database-url: cG9zdGdyZXNxbDovL3VzZXI6cGFzc3dvcmRAbG9jYWxob3N0OjU0MzIvZGI=
  api-key: eW91ci1hcGkta2V5LWhlcmU=
  config.json: ewogICJkYXRhYmFzZSI6IHsKICAgICJob3N0IjogImxvY2FsaG9zdCIsCiAgICAicG9ydCI6IDU0MzIKICB9Cn0=
immutable: true
```

## Mounting in Pods

The Secret is **automatically mounted** in pods when `enabled: true`. The secret is mounted as a **directory** at the specified mount path (default: `/etc/secrets`). **All secret keys become individual files** in this directory.

**Example**: If your secret has keys `{database-url: "...", api-key: "..."}`, they will be mounted as:
- `/etc/secrets/database-url`
- `/etc/secrets/api-key`

```yaml
secret:
  enabled: true
  name: "sequencer-secrets"
  type: Opaque
  stringData:
    database-url: "postgresql://user:password@localhost:5432/db"
    api-key: "your-api-key-here"
    config.json: |
      {
        "database": {
          "host": "localhost",
          "port": 5432
        }
      }
  # mountPath: /etc/secrets  # Optional: Override default mount path (default: "/etc/secrets")
```

The generated volume mount looks like:

```yaml
volumeMounts:
  - name: sequencer-secrets-volume
    mountPath: /etc/secrets  # All secret keys become files here
    readOnly: true
```

**Note**: If you also use ExternalSecret, make sure they use different mount paths to avoid conflicts.

## Automatic Container Arguments

When a Secret is enabled, the container **automatically receives** the `--config_file` argument pointing to the mounted secret directory:

```yaml
args:
  - --config_file
  - /config/sequencer/presets/  # from ConfigMap (always present)
  - --config_file
  - /etc/secrets   # from Secret (if enabled) - directory containing all secret files
```

Your application should then read the specific files it needs from this directory (e.g., `/etc/secrets/database-url`, `/etc/secrets/config.json`).

## Environment Variable Injection

You can also inject secret values as environment variables:

```yaml
env:
  - name: DATABASE_URL
    valueFrom:
      secretKeyRef:
        name: sequencer-secrets
        key: database-url
  - name: API_KEY
    valueFrom:
      secretKeyRef:
        name: sequencer-secrets
        key: api-key
```

## Best Practices

1. **Use `stringData`**: Prefer `stringData` over `data` for plain text values
2. **Immutable Secrets**: Use `immutable: true` for production secrets
3. **Encryption**: Use Sealed Secrets or external secret management
4. **Backup**: Enable backup for important secrets
5. **Labels**: Use consistent labeling for better resource management
6. **Access Control**: Implement proper RBAC for secret access
7. **Rotation**: Plan for secret rotation and updates
8. **Monitoring**: Monitor secret access and usage

## Common Use Cases

### Database Credentials

```yaml
secret:
  enabled: true
  name: "database-secrets"
  type: Opaque
  stringData:
    secrets.json: |
      {
        "database": {
          "username": "dbuser",
          "password": "securepassword",
          "host": "localhost",
          "port": "5432",
          "database": "myapp"
        }
      }
```

**Note**: When using auto-mounting, the secret must contain a key named `secret.json`. The file will be mounted at `/etc/secrets/secret.json`.

### API Keys

```yaml
secret:
  enabled: true
  name: "api-secrets"
  type: Opaque
  stringData:
    secrets.json: |
      {
        "api": {
          "stripe-key": "sk_test_...",
          "aws-access-key": "AKIA...",
          "aws-secret-key": "wJalrXUtn..."
        }
      }
```

**Note**: When using auto-mounting, the secret must contain a key named `secret.json`. The file will be mounted at `/etc/secrets/secret.json`.

### TLS Certificates

```yaml
secret:
  enabled: true
  name: "tls-secret"
  type: kubernetes.io/tls
  data:
    tls.crt: LS0tLS1CRUdJTi...  # Certificate
    tls.key: LS0tLS1CRUdJTi...  # Private key
```

### Configuration Files

```yaml
secret:
  enabled: true
  name: "config-secrets"
  type: Opaque
  stringData:
    app.properties: |
      database.url=jdbc:postgresql://localhost:5432/mydb
      database.username=myuser
      database.password=mypassword
    logging.xml: |
      <configuration>
        <appender name="FILE" class="ch.qos.logback.core.FileAppender">
          <file>/app/logs/app.log</file>
        </appender>
      </configuration>
```
