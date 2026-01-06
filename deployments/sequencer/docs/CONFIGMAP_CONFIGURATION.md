# ConfigMap Configuration Guide

This document describes all available configuration options for the ConfigMap construct.

## Basic Configuration

```yaml
config:
  configPaths:
    - "crates/apollo_deployments/resources/app_configs/base_layer_config.json"
    - "crates/apollo_deployments/resources/app_configs/sequencer_config.json"
```

## Advanced Configuration

```yaml
config:
  configPaths:
    - "crates/apollo_deployments/resources/app_configs/base_layer_config.json"
    - "crates/apollo_deployments/resources/app_configs/sequencer_config.json"
    - "crates/apollo_deployments/resources/app_configs/monitoring_config.json"
    - "crates/apollo_deployments/resources/app_configs/logging_config.json"
  # mountPath: /config/sequencer/presets/  # Optional: Override default mount path
```

## Configuration Options

### `config` (object)
- **Required**: Yes
- **Description**: Configuration for ConfigMap creation and mounting

#### `configPaths` (array of strings)
- **Required**: Yes
- **Description**: List of JSON configuration file paths to load and merge into the ConfigMap
- **Example**:
  ```yaml
  config:
    configPaths:
      - "crates/apollo_deployments/resources/app_configs/base_layer_config.json"
      - "crates/apollo_deployments/resources/app_configs/sequencer_config.json"
  ```

#### `mountPath` (string, optional)
- **Default**: `"/config/sequencer/presets/"`
- **Description**: Path where the config will be mounted in the container
- **Example**:
  ```yaml
  config:
    configPaths:
      - "crates/apollo_deployments/resources/app_configs/base_layer_config.json"
    mountPath: "/custom/config/path"
  ```

## File Path Resolution

The `configPaths` are resolved relative to the project root directory. The paths should be:

1. **Relative to project root**: All paths are relative to the main project directory
2. **JSON files only**: Only JSON files are supported for configuration
3. **Merged in order**: Files are loaded and merged in the order specified
4. **Error handling**: Missing files will cause deployment to fail

## Example Configuration Files

### base_layer_config.json
```json
{
  "logging": {
    "level": "info",
    "format": "json"
  },
  "database": {
    "host": "localhost",
    "port": 5432
  }
}
```

### sequencer_config.json
```json
{
  "sequencer": {
    "batch_size": 1000,
    "timeout": 30
  },
  "database": {
    "name": "sequencer_db"
  }
}
```

### monitoring_config.json
```json
{
  "monitoring": {
    "enabled": true,
    "port": 9090,
    "path": "/metrics"
  }
}
```

## Merged Configuration

The above configuration files would be merged into a single JSON object:

```json
{
  "logging": {
    "level": "info",
    "format": "json"
  },
  "database": {
    "host": "localhost",
    "port": 5432,
    "name": "sequencer_db"
  },
  "sequencer": {
    "batch_size": 1000,
    "timeout": 30
  },
  "monitoring": {
    "enabled": true,
    "port": 9090,
    "path": "/metrics"
  }
}
```

## Generated Kubernetes Resource

The configuration above generates a ConfigMap resource like this:

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: sequencer-node-config
  namespace: default
  labels:
    app: sequencer
    service: sequencer-node
data:
  config.json: |
    {
      "logging": {
        "level": "info",
        "format": "json"
      },
      "database": {
        "host": "localhost",
        "port": 5432,
        "name": "sequencer_db"
      },
      "sequencer": {
        "batch_size": 1000,
        "timeout": 30
      },
      "monitoring": {
        "enabled": true,
        "port": 9090,
        "path": "/metrics"
      }
    }
```

## Mounting in Pods

The ConfigMap is **automatically mounted** in pods when the `config` section is provided. The mount path defaults to `/config/sequencer/presets/` but can be customized:

```yaml
config:
  configPaths:
    - "crates/apollo_deployments/resources/app_configs/base_layer_config.json"
  mountPath: "/custom/config/path"  # Optional: defaults to "/config/sequencer/presets/"
```

The ConfigMap is mounted as a directory at the specified path. The generated volume mount looks like:

```yaml
volumeMounts:
  - name: sequencer-node-config
    mountPath: /config/sequencer/presets/  # or custom path
    readOnly: true
```

## Automatic Container Arguments

When a ConfigMap is configured, the container **automatically receives** the `--config_file` argument pointing to the mount path:

```yaml
args:
  - --config_file
  - /config/sequencer/presets/  # or custom mountPath if specified
```

This argument is always added first, before any additional arguments you may provide in the `args` section of `node.yaml`.

## Environment Variable Injection

You can also inject specific values as environment variables:

```yaml
env:
  - name: LOG_LEVEL
    valueFrom:
      configMapKeyRef:
        name: sequencer-node-config
        key: logging.level
  - name: DB_HOST
    valueFrom:
      configMapKeyRef:
        name: sequencer-node-config
        key: database.host
```

## Best Practices

1. **File Organization**: Organize configuration files by feature or environment
2. **Naming Convention**: Use descriptive names for configuration files
3. **Validation**: Validate JSON files before deployment
4. **Version Control**: Keep configuration files in version control
5. **Environment Separation**: Use different config files for different environments
6. **Sensitive Data**: Never put sensitive data in ConfigMaps (use Secrets instead)
7. **Documentation**: Document the structure of your configuration files

## Common Use Cases

### Environment-Specific Configuration

```yaml
# Development
config:
  configPaths:
    - "crates/apollo_deployments/resources/app_configs/base_layer_config.json"
    - "crates/apollo_deployments/resources/app_configs/sequencer_config.json"
    - "crates/apollo_deployments/resources/app_configs/dev_config.json"

# Production
config:
  configPaths:
    - "crates/apollo_deployments/resources/app_configs/base_layer_config.json"
    - "crates/apollo_deployments/resources/app_configs/sequencer_config.json"
    - "crates/apollo_deployments/resources/app_configs/prod_config.json"
```

### Feature-Based Configuration

```yaml
config:
  configPaths:
    - "crates/apollo_deployments/resources/app_configs/base_layer_config.json"
    - "crates/apollo_deployments/resources/app_configs/sequencer_config.json"
    - "crates/apollo_deployments/resources/app_configs/monitoring_config.json"
    - "crates/apollo_deployments/resources/app_configs/logging_config.json"
    - "crates/apollo_deployments/resources/app_configs/caching_config.json"
```

### Layered Configuration

```yaml
config:
  configPaths:
    - "crates/apollo_deployments/resources/app_configs/base_layer_config.json"      # Base configuration
    - "crates/apollo_deployments/resources/app_configs/sequencer_config.json"      # Service-specific
    - "crates/apollo_deployments/resources/app_configs/environment_config.json"    # Environment-specific
    - "crates/apollo_deployments/resources/app_configs/override_config.json"       # Overrides
  mountPath: "/config/sequencer/presets/"  # Optional: defaults to "/config/sequencer/presets/"
```
