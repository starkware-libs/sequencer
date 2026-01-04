# Ingress Configuration Guide

This document describes all available configuration options for the Ingress construct.

## Basic Configuration

```yaml
ingress:
  enabled: false
  className: ""
  annotations: {}
  hosts:
    - host: "sequencer.example.com"
      paths:
        - path: "/"
          pathType: "Prefix"
  tls: []
```

## Advanced Configuration

```yaml
ingress:
  enabled: true
  className: "nginx"
  annotations:
    "nginx.ingress.kubernetes.io/rewrite-target": "/"
    "nginx.ingress.kubernetes.io/ssl-redirect": "true"
    "cert-manager.io/cluster-issuer": "letsencrypt-prod"
    "nginx.ingress.kubernetes.io/rate-limit": "100"
  hosts:
    - host: "sequencer.example.com"
      paths:
        - path: "/"
          pathType: "Prefix"
        - path: "/api"
          pathType: "Prefix"
    - host: "api.sequencer.com"
      paths:
        - path: "/"
          pathType: "Prefix"
  tls:
    - secretName: "sequencer-tls"
      hosts:
        - "sequencer.example.com"
        - "api.sequencer.com"
  internal: false
  alternative_names: []
  rules: []
  cloud_armor_policy_name: ""
```

## Configuration Options

### `enabled` (boolean)
- **Default**: `false`
- **Description**: Whether to create the Ingress resource
- **Example**: `enabled: true` to enable Ingress creation

### `className` (string, optional)
- **Default**: `""`
- **Description**: Ingress class name (e.g., "nginx", "traefik", "istio")
- **Example**: `className: "nginx"`

### `annotations` (object)
- **Default**: `{}`
- **Description**: Annotations to add to the Ingress metadata
- **Common Annotations**:
  - **NGINX**: `"nginx.ingress.kubernetes.io/rewrite-target": "/"`
  - **Cert-Manager**: `"cert-manager.io/cluster-issuer": "letsencrypt-prod"`
  - **Rate Limiting**: `"nginx.ingress.kubernetes.io/rate-limit": "100"`
  - **SSL Redirect**: `"nginx.ingress.kubernetes.io/ssl-redirect": "true"`

### `hosts` (array of objects)
- **Required**: Yes (when enabled)
- **Description**: List of host configurations
- **Properties**:
  - `host` (string): Domain name
  - `paths` (array): List of path configurations
    - `path` (string): URL path
    - `pathType` (string): Path type ("Prefix", "Exact", "ImplementationSpecific")

### `tls` (array of objects, optional)
- **Default**: `[]`
- **Description**: TLS configuration for HTTPS
- **Properties**:
  - `secretName` (string): Name of the TLS secret
  - `hosts` (array): List of hostnames for this TLS configuration

### `internal` (boolean, optional)
- **Default**: `false`
- **Description**: Whether the ingress is internal-only
- **Example**: `internal: true` for internal services

### `alternative_names` (array of strings, optional)
- **Default**: `[]`
- **Description**: Alternative hostnames for the ingress
- **Example**: `["www.example.com", "app.example.com"]`

### `rules` (array of objects, optional)
- **Default**: `[]`
- **Description**: Custom ingress rules (advanced usage)
- **Example**: Custom rule configurations

### `cloud_armor_policy_name` (string, optional)
- **Default**: `""`
- **Description**: Google Cloud Armor policy name for security
- **Example**: `"my-security-policy"`

## Ingress Controller Examples

### NGINX Ingress Controller

```yaml
ingress:
  enabled: true
  className: "nginx"
  annotations:
    "nginx.ingress.kubernetes.io/rewrite-target": "/"
    "nginx.ingress.kubernetes.io/ssl-redirect": "true"
    "nginx.ingress.kubernetes.io/rate-limit": "100"
    "nginx.ingress.kubernetes.io/rate-limit-window": "1m"
    "cert-manager.io/cluster-issuer": "letsencrypt-prod"
  hosts:
    - host: "sequencer.example.com"
      paths:
        - path: "/"
          pathType: "Prefix"
  tls:
    - secretName: "sequencer-tls"
      hosts:
        - "sequencer.example.com"
```

### Traefik Ingress Controller

```yaml
ingress:
  enabled: true
  className: "traefik"
  annotations:
    "traefik.ingress.kubernetes.io/router.entrypoints": "websecure"
    "traefik.ingress.kubernetes.io/router.tls": "true"
    "cert-manager.io/cluster-issuer": "letsencrypt-prod"
  hosts:
    - host: "sequencer.example.com"
      paths:
        - path: "/"
          pathType: "Prefix"
  tls:
    - secretName: "sequencer-tls"
      hosts:
        - "sequencer.example.com"
```

### Istio Gateway (using VirtualService)

```yaml
ingress:
  enabled: true
  className: "istio"
  annotations:
    "istio.io/gateway-name": "sequencer-gateway"
  hosts:
    - host: "sequencer.example.com"
      paths:
        - path: "/"
          pathType: "Prefix"
```

## TLS Configuration Examples

### Let's Encrypt with Cert-Manager

```yaml
ingress:
  enabled: true
  className: "nginx"
  annotations:
    "cert-manager.io/cluster-issuer": "letsencrypt-prod"
    "nginx.ingress.kubernetes.io/ssl-redirect": "true"
  hosts:
    - host: "sequencer.example.com"
      paths:
        - path: "/"
          pathType: "Prefix"
  tls:
    - secretName: "sequencer-tls"
      hosts:
        - "sequencer.example.com"
```

### Custom TLS Certificate

```yaml
ingress:
  enabled: true
  className: "nginx"
  hosts:
    - host: "sequencer.example.com"
      paths:
        - path: "/"
          pathType: "Prefix"
  tls:
    - secretName: "custom-tls-secret"
      hosts:
        - "sequencer.example.com"
```

## Security Configuration

### Google Cloud Armor

```yaml
ingress:
  enabled: true
  className: "gce"
  annotations:
    "kubernetes.io/ingress.global-static-ip-name": "sequencer-ip"
    "networking.gke.io/managed-certificates": "sequencer-ssl-cert"
  hosts:
    - host: "sequencer.example.com"
      paths:
        - path: "/"
          pathType: "Prefix"
  cloud_armor_policy_name: "sequencer-security-policy"
```

### Rate Limiting

```yaml
ingress:
  enabled: true
  className: "nginx"
  annotations:
    "nginx.ingress.kubernetes.io/rate-limit": "100"
    "nginx.ingress.kubernetes.io/rate-limit-window": "1m"
    "nginx.ingress.kubernetes.io/rate-limit-connections": "10"
  hosts:
    - host: "sequencer.example.com"
      paths:
        - path: "/"
          pathType: "Prefix"
```

## Generated Kubernetes Resource

The configuration above generates an Ingress resource like this:

```yaml
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: sequencer-ingress
  namespace: default
  annotations:
    nginx.ingress.kubernetes.io/rewrite-target: /
    nginx.ingress.kubernetes.io/ssl-redirect: "true"
    cert-manager.io/cluster-issuer: letsencrypt-prod
spec:
  ingressClassName: nginx
  tls:
    - hosts:
        - sequencer.example.com
      secretName: sequencer-tls
  rules:
    - host: sequencer.example.com
      http:
        paths:
          - path: /
            pathType: Prefix
            backend:
              service:
                name: sequencer-service
                port:
                  number: 80
```

## Best Practices

1. **TLS**: Always use HTTPS in production
2. **Annotations**: Use controller-specific annotations for advanced features
3. **Path Types**: Choose appropriate path types ("Prefix" for most cases)
4. **Rate Limiting**: Implement rate limiting for public APIs
5. **Security**: Use security policies and WAF when available
6. **Monitoring**: Add monitoring annotations for observability
7. **Certificates**: Use cert-manager for automatic certificate management
