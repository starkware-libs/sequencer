# GCP PodMonitoring Configuration Guide

This document describes all available configuration options for the GCP PodMonitoring construct (Google Cloud Managed Prometheus).

> **Note**: PodMonitoring is a GCP-specific Custom Resource Definition (CRD) for Managed Prometheus on Google Kubernetes Engine (GKE).

## Basic Configuration

```yaml
gcpPodMonitoring:
  enabled: true
  name: ""  # Optional: defaults to sequencer-{service}-podmonitoring
  annotations: {}
  labels: {}
  spec:
    selector:
      matchLabels: {}  # Uses service labels if empty
      matchExpressions: []
    endpoints:
      - port: 9090  # Port name or number (required)
        path: "/monitoring/metrics"  # HTTP path (default: /metrics)
        interval: "10s"  # Scrape interval (Prometheus duration format)
        timeout: "5s"  # Scrape timeout (must be < interval)
        scheme: "http"  # Protocol scheme (http/https)
    filterRunning: true  # Filter out Failed/Succeeded pods
    limits: {}  # Optional: scrape limits
    targetLabels: {}  # Optional: labels to add to Prometheus targets
```

## Advanced Configuration

```yaml
gcpPodMonitoring:
  enabled: true
  name: "sequencer-custom-podmonitoring"
  annotations:
    "custom.annotation/key": "value"
  labels:
    "custom.label/key": "value"
  spec:
    selector:
      matchLabels:
        app: sequencer
        service: node
      matchExpressions:
        - key: environment
          operator: In
          values: ["production", "staging"]
    endpoints:
      - port: 9090
        path: "/monitoring/metrics"
        interval: "10s"
        timeout: "5s"
        scheme: "https"
        params:
          format: ["prometheus"]
          version: ["v1"]
        # Advanced endpoint options:
        # proxyUrl: "http://proxy:8080"
        # metricRelabeling: []
        # authorization: {}
        # basicAuth: {}
        # oauth2: {}
        # tls: {}
    filterRunning: true
    limits:
      samples: 1000000  # Max samples per scrape
      labels: 30  # Max labels per sample
      labelNameLength: 512  # Max label name length
      labelValueLength: 2048  # Max label value length
    targetLabels:
      metadata: ["pod", "container", "node"]
      fromPod:
        - from: "app.kubernetes.io/component"
          to: "component"
        - from: "app.kubernetes.io/version"
          to: "version"
```

## Configuration Options

### Top-Level Configuration

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `enabled` | `bool` | Yes | `false` | Enable PodMonitoring resource creation |
| `name` | `string` | No | `sequencer-{service}-podmonitoring` | Custom name for the PodMonitoring resource |
| `annotations` | `dict` | No | `{}` | Kubernetes annotations for the PodMonitoring resource |
| `labels` | `dict` | No | `{}` | Kubernetes labels for the PodMonitoring resource (merged with service labels) |
| `spec` | `object` | Yes | - | PodMonitoring specification |

### Spec Configuration

#### Selector

The selector determines which pods are monitored:

```yaml
spec:
  selector:
    matchLabels:
      app: sequencer
      service: node
    matchExpressions:
      - key: environment
        operator: In
        values: ["production"]
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `matchLabels` | `dict` | No | Label selector using exact match (uses service labels if empty) |
| `matchExpressions` | `list` | No | Advanced label selector with operators (In, NotIn, Exists, DoesNotExist) |

#### Endpoints

Configure the metrics endpoints to scrape:

```yaml
spec:
  endpoints:
    - port: 9090  # Required: port name or number
      path: "/metrics"  # Optional: HTTP path (default: /metrics)
      interval: "10s"  # Optional: scrape interval (default: 10s)
      timeout: "5s"  # Optional: scrape timeout
      scheme: "http"  # Optional: http or https
```

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `port` | `int\|string` | Yes | - | Port name or number to scrape |
| `path` | `string` | No | `/metrics` | HTTP path to scrape metrics from |
| `interval` | `string` | No | `10s` | Scrape interval (Prometheus duration: e.g., `30s`, `1m`) |
| `timeout` | `string` | No | - | Scrape timeout (must be less than interval) |
| `scheme` | `string` | No | `http` | Protocol scheme (`http` or `https`) |
| `params` | `dict` | No | - | HTTP GET parameters |
| `proxyUrl` | `string` | No | - | HTTP proxy URL (no encoded passwords) |

#### Advanced Endpoint Options

```yaml
endpoints:
  - port: 9090
    # Metric relabeling rules
    metricRelabeling:
      - action: drop
        regex: ".*debug.*"
        sourceLabels: ["__name__"]
    
    # HTTP authorization
    authorization:
      type: "Bearer"
      credentials:
        secret:
          name: "auth-token"
          key: "token"
    
    # HTTP basic authentication
    basicAuth:
      username: "monitoring"
      password:
        secret:
          name: "basic-auth"
          key: "password"
    
    # OAuth2 authentication
    oauth2:
      clientID: "client-id"
      clientSecret:
        secret:
          name: "oauth-secret"
          key: "client-secret"
      tokenURL: "https://oauth.example.com/token"
      scopes: ["read", "monitoring"]
    
    # TLS configuration
    tls:
      ca:
        secret:
          name: "ca-cert"
          key: "ca.crt"
      cert:
        secret:
          name: "client-cert"
          key: "cert.pem"
      key:
        secret:
          name: "client-key"
          key: "key.pem"
      insecureSkipVerify: false
      serverName: "metrics.example.com"
```

| Field | Type | Description |
|-------|------|-------------|
| `metricRelabeling` | `list` | Prometheus metric relabeling rules (cannot override protected labels) |
| `authorization` | `object` | HTTP authorization credentials (Bearer token) |
| `basicAuth` | `object` | HTTP basic authentication |
| `oauth2` | `object` | OAuth2 client credentials |
| `tls` | `object` | TLS configuration for secure scraping |

#### Filter Running

```yaml
spec:
  filterRunning: true  # Filter out Failed/Succeeded pods
```

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `filterRunning` | `bool` | No | `true` | Filter out pods in "Failed" or "Succeeded" phase |

#### Limits

Configure scrape limits to protect Prometheus:

```yaml
spec:
  limits:
    samples: 1000000  # Max samples per scrape
    labels: 30  # Max labels per sample
    labelNameLength: 512  # Max label name length
    labelValueLength: 2048  # Max label value length
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `samples` | `int` | No | Maximum samples accepted per scrape |
| `labels` | `int` | No | Maximum labels accepted per sample |
| `labelNameLength` | `int` | No | Maximum label name length |
| `labelValueLength` | `int` | No | Maximum label value length |

#### Target Labels

Add custom labels to Prometheus targets:

```yaml
spec:
  targetLabels:
    metadata: ["pod", "container", "node"]  # Pod metadata labels
    fromPod:
      - from: "app.kubernetes.io/component"
        to: "component"
      - from: "app.kubernetes.io/version"
        to: "version"
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `metadata` | `list` | No | Pod metadata labels to include: `pod`, `container`, `node`, `namespace` |
| `fromPod` | `list` | No | Label mappings from pod labels to Prometheus target labels |

## GKE-Specific Examples

### Simple Monitoring Configuration

```yaml
gcpPodMonitoring:
  enabled: true
  spec:
    selector:
      matchLabels:
        app: sequencer
        service: node
    endpoints:
      - port: 9090
        path: "/monitoring/metrics"
        interval: "15s"
```

### Monitoring with TLS

```yaml
gcpPodMonitoring:
  enabled: true
  spec:
    selector:
      matchLabels:
        app: sequencer
    endpoints:
      - port: 9443
        path: "/metrics"
        scheme: "https"
        tls:
          ca:
            secret:
              name: "metrics-ca"
              key: "ca.crt"
          insecureSkipVerify: false
```

### Monitoring with Authentication

```yaml
gcpPodMonitoring:
  enabled: true
  spec:
    selector:
      matchLabels:
        app: sequencer
    endpoints:
      - port: 9090
        path: "/metrics"
        basicAuth:
          username: "monitoring"
          password:
            secret:
              name: "metrics-auth"
              key: "password"
```

### Multiple Endpoints

```yaml
gcpPodMonitoring:
  enabled: true
  spec:
    selector:
      matchLabels:
        app: sequencer
    endpoints:
      - port: 9090
        path: "/metrics"
        interval: "10s"
      - port: 9091
        path: "/custom-metrics"
        interval: "30s"
```

## Best Practices

1. **Resource Naming**: Use descriptive names that identify the service and environment
2. **Scrape Intervals**: Use appropriate intervals (10-30s for production, 60s for development)
3. **Filter Running**: Always enable `filterRunning` to avoid scraping terminated pods
4. **Limits**: Set appropriate limits to protect Prometheus from excessive metrics
5. **Authentication**: Use TLS and authentication for production environments
6. **Label Selectors**: Use specific label selectors to target only the pods you want to monitor
7. **Target Labels**: Use `targetLabels` to add meaningful context to your metrics

## Generated Kubernetes Resource

The configuration generates a PodMonitoring Custom Resource:

```yaml
apiVersion: monitoring.googleapis.com/v1
kind: PodMonitoring
metadata:
  name: sequencer-node-podmonitoring
  namespace: default
  labels:
    app: sequencer
    service: sequencer-node
spec:
  selector:
    matchLabels:
      app: sequencer
      service: sequencer-node
  endpoints:
    - port: 9090
      path: /monitoring/metrics
      interval: 10s
      timeout: 5s
      scheme: http
  filterRunning: true
```

## References

- [GCP Managed Prometheus Documentation](https://cloud.google.com/stackdriver/docs/managed-prometheus)
- [PodMonitoring CRD Specification](https://cloud.google.com/kubernetes-engine/docs/how-to/managed-prometheus)

