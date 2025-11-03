# Layout and Overlay Configuration Guide

This guide explains how the layout and overlay mechanism works for managing Kubernetes deployment configurations across different environments.

## Overview

The configuration system uses a **two-tier approach**:
1. **Layouts**: Base configurations that define the structure and default values
2. **Overlays**: Environment-specific overrides that modify layout configurations

This approach allows you to:
- Define configurations once in a layout
- Customize them per environment (dev, integration, prod) via overlays
- Ensure consistency while allowing environment-specific differences

## Directory Structure

```
configs/
├── layouts/                    # Base configurations
│   └── consolidated/          # Layout name (e.g., consolidated, hybrid, distributed)
│       ├── common.yaml        # Common configuration (applies to all services)
│       └── services/          # Service-specific configurations
│           └── node.yaml      # Service configuration file
│
└── overlays/                   # Environment-specific overrides
    └── consolidated/          # Must match layout name
        └── dev/               # Overlay name (e.g., dev, integration, prod)
            ├── common.yaml    # Common overlay configuration
            └── services/      # Service-specific overlays
                └── node.yaml  # Service overlay configuration
```

## How It Works

### 1. Layout Configuration

**Layouts** define the base structure and default values for your deployments. Each layout:
- Defines all available configuration keys
- Sets default values for your services
- Establishes the "schema" that overlays must follow

Example layout (`configs/layouts/consolidated/services/node.yaml`):
```yaml
name: node

# StatefulSet Configuration
statefulSet:
  enabled: true
  replicas: 3
  annotations: {}
  labels: {}

# Service Configuration
service:
  enabled: true
  type: ClusterIP
  ports:
    - name: http
      port: 80
      targetPort: 8080

# Ingress Configuration
ingress:
  enabled: true
  ingressClassName: "nginx"
  hosts:
    - sequencer.example.com
```

### 2. Overlay Configuration

**Overlays** modify existing layout values. They:
- Can only modify keys that exist in the layout
- Cannot add new keys or services
- Support nested merging for complex objects
- Overwrite primitive values (strings, numbers, booleans)

Example overlay (`configs/overlays/consolidated/dev/services/node.yaml`):
```yaml
name: node

# Disable StatefulSet in dev (override layout's enabled: true)
statefulSet:
  enabled: false

# Override service ports
service:
  ports:
    - name: http
      port: 8080
      targetPort: 8080

# Disable ingress in dev
ingress:
  enabled: false
```

### 3. Merging Process

The merge process works as follows:

1. **Load layout configuration** from `configs/layouts/<layout>/`
2. **Load overlay configuration** (if provided) from `configs/overlays/<layout>/<overlay>/`
3. **Strict merge**: Overlay values are merged into layout values with validation
4. **Validation**: System ensures overlay doesn't add new keys or services

#### Merge Rules

- **Nested objects**: Merged recursively (overlay values update layout values)
- **Primitive values**: Completely replaced (layout value is overwritten)
- **Arrays**: Completely replaced (layout array is overwritten)
- **Missing keys in overlay**: Layout values are preserved

#### Example Merge

**Layout:**
```yaml
service:
  enabled: true
  type: ClusterIP
  ports:
    - name: http
      port: 80
    - name: monitoring
      port: 9090
```

**Overlay:**
```yaml
service:
  type: LoadBalancer
  ports:
    - name: http
      port: 8080
```

**Result (merged):**
```yaml
service:
  enabled: true          # From layout (not in overlay)
  type: LoadBalancer     # From overlay (overwrites layout)
  ports:
    - name: http
      port: 8080          # From overlay (array replaced)
    # monitoring port removed (array was replaced, not merged)
```

## Usage

### Command-Line Interface

Use the `--layout` and `--overlay` flags to specify which configurations to use:

```bash
# Use layout only (no overlay)
cdk8s synth --app "pipenv run python -m main --namespace production --layout consolidated"

# Use layout with dev overlay
cdk8s synth --app "pipenv run python -m main --namespace dev --layout consolidated --overlay dev"

# Use layout with prod overlay
cdk8s synth --app "pipenv run python -m main --namespace prod --layout consolidated --overlay prod"
```

### Available Layouts

- `consolidated`: All services in a single namespace
- `hybrid`: Mix of consolidated and distributed services
- `distributed`: Services spread across multiple namespaces

### Available Overlays

- `dev`: Development environment (typically disables production features)
- `integration`: Integration/staging environment
- `prod`: Production environment (full feature set)

## Strict Validation

The overlay system uses **strict validation** to ensure consistency:

### What's Allowed ✅

- Modifying existing keys
- Changing values of existing keys
- Disabling features (e.g., `enabled: false`)
- Overriding nested configuration values

### What's Not Allowed ❌

- Adding new keys not present in layout
- Introducing new services not in layout
- Adding new fields to nested objects not in layout

### Error Messages

If you try to add a new key, you'll see an error like:
```
ValueError: ❌ Overlay file 'configs/overlays/consolidated/dev/services/node.yaml' tried to add new key 'newFeature.enabled'
```

If you try to add a new service, you'll see:
```
ValueError: ❌ Overlay tried to introduce new service 'newService' not in layout
```

## Common Patterns

### Pattern 1: Disable Resources in Dev

**Layout:**
```yaml
ingress:
  enabled: true
```

**Overlay (dev):**
```yaml
ingress:
  enabled: false
```

### Pattern 2: Override Image Tags

**Layout:**
```yaml
image:
  repository: ghcr.io/starkware-libs/sequencer
  tag: "latest"
```

**Overlay (dev):**
```yaml
image:
  tag: "dev"
```

### Pattern 3: Modify Resource Limits

**Layout:**
```yaml
resources:
  requests:
    cpu: "1000m"
    memory: "2Gi"
  limits:
    cpu: "2000m"
    memory: "4Gi"
```

**Overlay (dev):**
```yaml
resources:
  requests:
    cpu: "100m"
    memory: "512Mi"
  limits:
    cpu: "500m"
    memory: "1Gi"
```

### Pattern 4: Change Service Ports

**Layout:**
```yaml
service:
  ports:
    - name: http
      port: 80
      targetPort: 8080
```

**Overlay (dev):**
```yaml
service:
  ports:
    - name: http
      port: 8080
      targetPort: 8080
```

## Common Configuration (`common.yaml`)

Both layouts and overlays can have a `common.yaml` file that applies to all services:

### Layout Common (`configs/layouts/<layout>/common.yaml`)
```yaml
image:
  repository: ghcr.io/starkware-libs/sequencer
  tag: "latest"
  imagePullPolicy: IfNotPresent

imagePullSecrets: []
commonMetaLabels: {}
```

### Overlay Common (`configs/overlays/<layout>/<overlay>/common.yaml`)
```yaml
image:
  tag: "dev"  # Override tag for dev environment
```

## Best Practices

1. **Keep layouts comprehensive**: Define all configuration keys in layouts, even with default values
2. **Use overlays sparingly**: Only override what's necessary for each environment
3. **Document overrides**: Add comments in overlay files explaining why certain overrides exist
4. **Test merges**: Run `cdk8s synth` to verify overlays work correctly before deploying
5. **Version control**: Keep both layouts and overlays in version control for reproducibility

## Troubleshooting

### Overlay Not Applying

If your overlay changes aren't appearing:
1. Verify the overlay file path matches: `configs/overlays/<layout>/<overlay>/services/<service>.yaml`
2. Check you're using the correct `--overlay` flag value
3. Ensure the service name matches exactly between layout and overlay
4. Run with verbose logging to see what files are being loaded

### Validation Errors

If you see validation errors:
1. Check that all keys in overlay exist in layout
2. Verify service names match between layout and overlay
3. Ensure you're not trying to add new services in overlay

### Common Mistakes

- **Adding new keys in overlay**: Only modify existing keys
- **Mismatched service names**: Service name in overlay must match layout exactly
- **Incorrect path structure**: Overlay path must mirror layout structure
- **Array merging**: Remember arrays are replaced, not merged

## Related Documentation

- [Service Configuration](SERVICE_CONFIGURATION.md)
- [StatefulSet Configuration](STATEFULSET_CONFIGURATION.md)
- [Ingress Configuration](INGRESS_CONFIGURATION.md)
- See [README.md](README.md) for all available configuration documentation

