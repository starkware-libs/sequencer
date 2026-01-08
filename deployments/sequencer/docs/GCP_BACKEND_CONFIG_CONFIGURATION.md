# GCP BackendConfig Configuration Guide

This document describes all available configuration options for the GCP BackendConfig construct (Google Cloud Load Balancer).

> **Note**: BackendConfig is a GCP-specific Custom Resource Definition (CRD) for configuring Google Cloud Load Balancer backend services. It only works with GKE Ingress.

## Basic Configuration

```yaml
gcpBackendConfig:
  enabled: true
  customRequestHeaders: []
  connectionDrainingTimeoutSeconds: 0
  securityPolicy: ""
  timeOutSeconds: 0
  healthCheck:
    checkIntervalSeconds: 0
    timeoutSeconds: 0
    healthyThreshold: 0
    unhealthyThreshold: 0
    requestPath: ""
    port: 0
```

## Advanced Configuration

```yaml
gcpBackendConfig:
  enabled: true
  customRequestHeaders:
    - "X-Custom-Header: value"
    - "X-Request-ID: ${request_id}"
  connectionDrainingTimeoutSeconds: 60  # Time to wait before draining connections
  securityPolicy: "gcp-cloud-armor-policy"  # Cloud Armor security policy
  timeOutSeconds: 30  # Request timeout
  healthCheck:
    checkIntervalSeconds: 10  # Health check interval
    timeoutSeconds: 5  # Health check timeout
    healthyThreshold: 2  # Consecutive successful checks
    unhealthyThreshold: 3  # Consecutive failed checks
    requestPath: "/health"  # Health check path
    port: 80  # Health check port
```

## Configuration Options

### Top-Level Configuration

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `enabled` | `bool` | Yes | `false` | Enable BackendConfig resource creation |
| `customRequestHeaders` | `list` | No | `[]` | Custom HTTP headers to add to requests |
| `connectionDrainingTimeoutSeconds` | `int` | No | `0` | Seconds to wait before draining connections during shutdown |
| `securityPolicy` | `string` | No | `""` | Cloud Armor security policy name |
| `timeOutSeconds` | `int` | No | `0` | Request timeout in seconds |

### Health Check Configuration

```yaml
gcpBackendConfig:
  healthCheck:
    checkIntervalSeconds: 10  # How often to check (seconds)
    timeoutSeconds: 5  # How long to wait for response (seconds)
    healthyThreshold: 2  # Number of consecutive successful checks
    unhealthyThreshold: 3  # Number of consecutive failed checks
    requestPath: "/health"  # HTTP path for health checks
    port: 80  # Port to use for health checks
```

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `checkIntervalSeconds` | `int` | No | `0` | Interval between health checks (seconds) |
| `timeoutSeconds` | `int` | No | `0` | Timeout for each health check (seconds) |
| `healthyThreshold` | `int` | No | `0` | Consecutive successful checks before marking healthy |
| `unhealthyThreshold` | `int` | No | `0` | Consecutive failed checks before marking unhealthy |
| `requestPath` | `string` | No | `""` | HTTP path for health check requests |
| `port` | `int` | No | `0` | Port number for health checks |

## GKE-Specific Examples

### Basic BackendConfig with Health Checks

```yaml
gcpBackendConfig:
  enabled: true
  healthCheck:
    checkIntervalSeconds: 10
    timeoutSeconds: 5
    healthyThreshold: 2
    unhealthyThreshold: 3
    requestPath: "/health"
    port: 80
```

### BackendConfig with Custom Headers

```yaml
gcpBackendConfig:
  enabled: true
  customRequestHeaders:
    - "X-Forwarded-For: ${client_ip}"
    - "X-Request-ID: ${request_id}"
    - "X-Environment: production"
  connectionDrainingTimeoutSeconds: 60
  timeOutSeconds: 30
  healthCheck:
    checkIntervalSeconds: 15
    timeoutSeconds: 5
    healthyThreshold: 2
    unhealthyThreshold: 3
    requestPath: "/ready"
    port: 8080
```

### BackendConfig with Cloud Armor

```yaml
gcpBackendConfig:
  enabled: true
  securityPolicy: "gcp-cloud-armor-policy-name"
  connectionDrainingTimeoutSeconds: 90
  timeOutSeconds: 45
  healthCheck:
    checkIntervalSeconds: 10
    timeoutSeconds: 5
    healthyThreshold: 2
    unhealthyThreshold: 3
    requestPath: "/health"
    port: 80
```

### Advanced Health Check Configuration

```yaml
gcpBackendConfig:
  enabled: true
  customRequestHeaders:
    - "X-Custom-Header: value"
  connectionDrainingTimeoutSeconds: 120
  timeOutSeconds: 60
  healthCheck:
    checkIntervalSeconds: 5  # More frequent checks
    timeoutSeconds: 3  # Faster timeout
    healthyThreshold: 1  # Mark healthy after 1 success
    unhealthyThreshold: 5  # Mark unhealthy after 5 failures
    requestPath: "/api/health"
    port: 8080
```

## Custom Request Headers

BackendConfig supports dynamic header values using variables:

```yaml
gcpBackendConfig:
  customRequestHeaders:
    - "X-Client-IP: ${client_ip}"
    - "X-Request-ID: ${request_id}"
    - "X-Protocol: ${protocol}"
    - "X-Load-Balancer: ${lb_ip}"
```

Available variables:
- `${client_ip}` - Client IP address
- `${request_id}` - Unique request ID
- `${protocol}` - Protocol (HTTP/HTTPS)
- `${lb_ip}` - Load balancer IP

## Integration with Ingress

BackendConfig must be referenced from an Ingress resource:

```yaml
ingress:
  enabled: true
  annotations:
    "cloud.google.com/backend-config": '{"default": "sequencer-backend-config"}'
```

The BackendConfig name follows the pattern: `sequencer-{service}-backendconfig`

## Best Practices

1. **Health Checks**: Configure appropriate health check intervals and thresholds
   - Production: 10-15s interval, 2-3 threshold
   - Development: 30s interval, 1 threshold
2. **Connection Draining**: Set reasonable timeout (60-120s) to allow graceful shutdowns
3. **Request Timeout**: Set appropriate timeout based on application response times
4. **Security Policy**: Use Cloud Armor for production workloads
5. **Custom Headers**: Use custom headers for request tracking and debugging
6. **Health Check Path**: Use dedicated health check endpoints (not application endpoints)

## Generated Kubernetes Resource

The configuration generates a BackendConfig Custom Resource:

```yaml
apiVersion: cloud.google.com/v1
kind: BackendConfig
metadata:
  name: sequencer-node-backendconfig
  namespace: default
spec:
  timeoutSec: 30
  connectionDraining:
    drainingTimeoutSec: 60
  healthCheck:
    checkIntervalSec: 10
    timeoutSec: 5
    healthyThreshold: 2
    unhealthyThreshold: 3
    type: HTTP
    requestPath: /health
    port: 80
  customRequestHeaders:
    headers:
      - "X-Custom-Header: value"
```

## References

- [GCP BackendConfig Documentation](https://cloud.google.com/kubernetes-engine/docs/how-to/ingress-features#backendconfig)
- [Cloud Armor Documentation](https://cloud.google.com/armor)
- [GKE Ingress Documentation](https://cloud.google.com/kubernetes-engine/docs/concepts/ingress)

