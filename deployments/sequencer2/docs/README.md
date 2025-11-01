# Configuration Documentation

This directory contains comprehensive documentation for all Kubernetes manifest configurations available in the Sequencer deployment system.

## Available Documentation

### Core Kubernetes Resources

- **[SERVICE_ACCOUNT_CONFIGURATION.md](SERVICE_ACCOUNT_CONFIGURATION.md)** - ServiceAccount configuration options including cloud provider integrations (GKE, EKS, AKS)
- **[SECRET_CONFIGURATION.md](SECRET_CONFIGURATION.md)** - Secret configuration for various types (Opaque, TLS, Docker, Basic Auth, SSH)
- **[EXTERNAL_SECRET_CONFIGURATION.md](EXTERNAL_SECRET_CONFIGURATION.md)** - ExternalSecret configuration for multiple secret providers (GCP, AWS, Azure, Vault)
- **[INGRESS_CONFIGURATION.md](INGRESS_CONFIGURATION.md)** - Ingress configuration for various controllers (NGINX, Traefik, Istio)
- **[STATEFULSET_CONFIGURATION.md](STATEFULSET_CONFIGURATION.md)** - StatefulSet configuration with advanced scheduling and security options
- **[DEPLOYMENT_CONFIGURATION.md](DEPLOYMENT_CONFIGURATION.md)** - Deployment configuration with scaling and update strategies
- **[CONFIGMAP_CONFIGURATION.md](CONFIGMAP_CONFIGURATION.md)** - ConfigMap configuration for JSON file loading and merging
- **[SERVICE_CONFIGURATION.md](SERVICE_CONFIGURATION.md)** - Service configuration for different types and cloud load balancers
- **[VOLUME_CONFIGURATION.md](VOLUME_CONFIGURATION.md)** - PersistentVolume configuration for various storage classes
- **[POD_DISRUPTION_BUDGET_CONFIGURATION.md](POD_DISRUPTION_BUDGET_CONFIGURATION.md)** - PodDisruptionBudget configuration for pod availability during disruptions

### GCP-Specific Resources

- **[GCP_POD_MONITORING_CONFIGURATION.md](GCP_POD_MONITORING_CONFIGURATION.md)** - GCP PodMonitoring configuration for Managed Prometheus on GKE
- **[GCP_BACKEND_CONFIG_CONFIGURATION.md](GCP_BACKEND_CONFIG_CONFIGURATION.md)** - GCP BackendConfig configuration for Google Cloud Load Balancer

### Specialized Features

- **[HPA_FLEXIBILITY_GUIDE.md](HPA_FLEXIBILITY_GUIDE.md)** - HorizontalPodAutoscaler configuration with advanced scaling behaviors

## Usage

Each documentation file provides:

1. **Basic Configuration** - Simple examples to get started
2. **Advanced Configuration** - Complex examples with all options
3. **Configuration Options** - Detailed explanation of each field
4. **Cloud Provider Examples** - Specific examples for AWS, GCP, Azure
5. **Best Practices** - Recommended approaches and security considerations
6. **Generated Kubernetes Resources** - Example of the actual YAML output

## Quick Reference

### ServiceAccount
```yaml
serviceAccount:
  enabled: true
  name: "my-sa"
  annotations:
    "iam.gke.io/gcp-service-account": "my-service@project.iam.gserviceaccount.com"
```

### Secret
```yaml
secret:
  enabled: true
  name: "app-secrets"
  type: Opaque
  stringData:
    database-url: "postgresql://user:password@localhost:5432/db"
    api-key: "your-api-key-here"
```

### ExternalSecret
```yaml
externalSecret:
  enabled: true
  secretStore:
    name: "gcp-secret-store"
    kind: "ClusterSecretStore"
  data:
    - secretKey: "database-url"
      remoteKey: "sequencer/database-url"
```

### Ingress
```yaml
ingress:
  enabled: true
  className: "nginx"
  hosts:
    - "sequencer.example.com"
  tls:
    - secretName: "sequencer-tls"
      hosts: ["sequencer.example.com"]
```

### StatefulSet
```yaml
statefulSet:
  enabled: true
  replicas: 3
  updateStrategy:
    type: "RollingUpdate"
  securityContext:
    runAsNonRoot: true
    runAsUser: 1000
```

### HPA
```yaml
hpa:
  enabled: true
  minReplicas: 3
  maxReplicas: 10
  targetCPUUtilizationPercentage: 70
```

### PodDisruptionBudget
```yaml
podDisruptionBudget:
  enabled: true
  maxUnavailable: "25%"
  unhealthyPodEvictionPolicy: "IfHealthyBudget"
```

### GCP PodMonitoring
```yaml
gcpPodMonitoring:
  enabled: true
  spec:
    selector:
      matchLabels:
        app: sequencer
    endpoints:
      - port: 9090
        path: "/monitoring/metrics"
        interval: "10s"
```

### GCP BackendConfig
```yaml
gcpBackendConfig:
  enabled: true
  connectionDrainingTimeoutSeconds: 60
  timeOutSeconds: 30
  healthCheck:
    checkIntervalSeconds: 10
    timeoutSeconds: 5
    healthyThreshold: 2
    unhealthyThreshold: 3
    requestPath: "/health"
    port: 80
```

## Contributing

When adding new configuration options:

1. Update the relevant documentation file
2. Add examples for different use cases
3. Include cloud provider-specific examples
4. Update this README with the new documentation
5. Test the configuration with `cdk8s synth`

## Schema Validation

All configurations are validated against Pydantic schemas defined in `src/config/schema.py`. The documentation reflects the actual schema structure and available options.
