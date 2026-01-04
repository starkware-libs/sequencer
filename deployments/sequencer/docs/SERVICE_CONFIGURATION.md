# Service Configuration Guide

This document describes all available configuration options for the Service construct.

## Basic Configuration

```yaml
service:
  type: "ClusterIP"
  ports:
    - name: "http"
      port: 80
      targetPort: 8080
      protocol: "TCP"
  annotations: {}
  labels: {}
```

## Advanced Configuration

```yaml
service:
  type: "LoadBalancer"
  ports:
    - name: "http"
      port: 80
      targetPort: 8080
      protocol: "TCP"
    - name: "https"
      port: 443
      targetPort: 8443
      protocol: "TCP"
    - name: "metrics"
      port: 9090
      targetPort: 9090
      protocol: "TCP"
  annotations:
    "service.beta.kubernetes.io/aws-load-balancer-type": "nlb"
    "service.beta.kubernetes.io/aws-load-balancer-scheme": "internet-facing"
  labels:
    "app.kubernetes.io/component": "service"
  externalTrafficPolicy: "Local"
  sessionAffinity: "ClientIP"
  sessionAffinityConfig:
    clientIP:
      timeoutSeconds: 3600
```

## Configuration Options

### `type` (string)
- **Default**: `"ClusterIP"`
- **Description**: Service type
- **Values**: `"ClusterIP"`, `"NodePort"`, `"LoadBalancer"`, `"ExternalName"`
- **Example**: `type: "LoadBalancer"` for external access

### `ports` (array of objects)
- **Required**: Yes
- **Description**: List of port configurations
- **Properties**:
  - `name` (string): Port name
  - `port` (integer): Service port
  - `targetPort` (integer): Target port on pods
  - `protocol` (string): Protocol ("TCP" or "UDP")
  - `nodePort` (integer, optional): Node port (for NodePort type)

### `annotations` (object)
- **Default**: `{}`
- **Description**: Annotations to add to the Service metadata
- **Common Annotations**:
  - **AWS Load Balancer**: `"service.beta.kubernetes.io/aws-load-balancer-type": "nlb"`
  - **GCP Load Balancer**: `"cloud.google.com/load-balancer-type": "External"`
  - **Azure Load Balancer**: `"service.beta.kubernetes.io/azure-load-balancer-internal": "true"`

### `labels` (object)
- **Default**: `{}`
- **Description**: Labels to add to the Service metadata
- **Example**:
  ```yaml
  labels:
    "app.kubernetes.io/component": "service"
    "app.kubernetes.io/part-of": "sequencer"
  ```

### `name` (string, optional)
- **Description**: Name suffix for extra services (only used in `extraServices` list)
- **Example**: `name: "gateway"` creates service `sequencer-{service-name}-gateway-service`
- **Note**: This field is only relevant when defining services in the `extraServices` list

### `extraServices` (array of Service objects, optional)
- **Default**: `[]` (empty list)
- **Description**: List of additional service definitions for the same deployment
- **Use Case**: Create multiple services with different types, ports, or annotations for the same pods
- **Example**: See [Multiple Services Per Deployment](#multiple-services-per-deployment) section

### `externalTrafficPolicy` (string, optional)
- **Default**: `"Cluster"`
- **Description**: External traffic policy for LoadBalancer and NodePort services
- **Values**: `"Cluster"` or `"Local"`

### `sessionAffinity` (string, optional)
- **Default**: `"None"`
- **Description**: Session affinity for the service
- **Values**: `"None"` or `"ClientIP"`

### `sessionAffinityConfig` (object, optional)
- **Default**: `{}`
- **Description**: Session affinity configuration
- **Example**:
  ```yaml
  sessionAffinityConfig:
    clientIP:
      timeoutSeconds: 3600
  ```

## Service Type Examples

### ClusterIP (Default)

```yaml
service:
  type: "ClusterIP"
  ports:
    - name: "http"
      port: 80
      targetPort: 8080
      protocol: "TCP"
```

### NodePort

```yaml
service:
  type: "NodePort"
  ports:
    - name: "http"
      port: 80
      targetPort: 8080
      nodePort: 30080
      protocol: "TCP"
```

### LoadBalancer

```yaml
service:
  type: "LoadBalancer"
  ports:
    - name: "http"
      port: 80
      targetPort: 8080
      protocol: "TCP"
  externalTrafficPolicy: "Local"
```

### ExternalName

```yaml
service:
  type: "ExternalName"
  externalName: "my-external-service.example.com"
  ports:
    - name: "http"
      port: 80
      protocol: "TCP"
```

## Cloud Provider Load Balancer Examples

### AWS Network Load Balancer

```yaml
service:
  type: "LoadBalancer"
  ports:
    - name: "http"
      port: 80
      targetPort: 8080
      protocol: "TCP"
  annotations:
    "service.beta.kubernetes.io/aws-load-balancer-type": "nlb"
    "service.beta.kubernetes.io/aws-load-balancer-scheme": "internet-facing"
    "service.beta.kubernetes.io/aws-load-balancer-cross-zone-load-balancing-enabled": "true"
  externalTrafficPolicy: "Local"
```

### GCP Load Balancer

```yaml
service:
  type: "LoadBalancer"
  ports:
    - name: "http"
      port: 80
      targetPort: 8080
      protocol: "TCP"
  annotations:
    "cloud.google.com/load-balancer-type": "External"
    "cloud.google.com/neg": '{"ingress": true}'
  externalTrafficPolicy: "Local"
```

### Azure Load Balancer

```yaml
service:
  type: "LoadBalancer"
  ports:
    - name: "http"
      port: 80
      targetPort: 8080
      protocol: "TCP"
  annotations:
    "service.beta.kubernetes.io/azure-load-balancer-internal": "true"
    "service.beta.kubernetes.io/azure-load-balancer-internal-subnet": "my-subnet"
  externalTrafficPolicy: "Local"
```

## Multi-Port Configuration

### HTTP and HTTPS

```yaml
service:
  type: "LoadBalancer"
  ports:
    - name: "http"
      port: 80
      targetPort: 8080
      protocol: "TCP"
    - name: "https"
      port: 443
      targetPort: 8443
      protocol: "TCP"
```

### HTTP, HTTPS, and Metrics

```yaml
service:
  type: "ClusterIP"
  ports:
    - name: "http"
      port: 80
      targetPort: 8080
      protocol: "TCP"
    - name: "https"
      port: 443
      targetPort: 8443
      protocol: "TCP"
    - name: "metrics"
      port: 9090
      targetPort: 9090
      protocol: "TCP"
```

## Session Affinity Configuration

### Client IP Session Affinity

```yaml
service:
  type: "ClusterIP"
  ports:
    - name: "http"
      port: 80
      targetPort: 8080
      protocol: "TCP"
  sessionAffinity: "ClientIP"
  sessionAffinityConfig:
    clientIP:
      timeoutSeconds: 3600
```

## Multiple Services Per Deployment

You can create multiple Kubernetes services for the same deployment using the `extraServices` field. This is useful when you need to expose different ports via different service types (e.g., ClusterIP with Ingress for public access, LoadBalancer for internal access).

### Basic Example

```yaml
# Primary service (exposed via Ingress)
service:
  type: "ClusterIP"
  ports:
    - name: "http-server"
      port: 8080
      targetPort: 8080
      protocol: "TCP"
  annotations:
    cloud.google.com/neg: '{"ingress": true}'

# Extra service (internal LoadBalancer)
extraServices:
  - name: "gateway"  # Optional: used as suffix in service name
    type: "LoadBalancer"
    ports:
      - name: "gateway"
        port: 55002
        targetPort: 55002
        protocol: "TCP"
    annotations:
      cloud.google.com/load-balancer-type: "Internal"
      networking.gke.io/internal-load-balancer-allow-global-access: "true"
```

This configuration generates:
- Primary service: `sequencer-httpserver-service` (ClusterIP, http-server port)
- Extra service: `sequencer-httpserver-gateway-service` (LoadBalancer, gateway port)

Both services target the same pods using the same pod selector (labels).

### Use Cases

1. **Unified Deployment with Multiple Exposures**: Combine http-server and gateway into one deployment
   - http-server: Exposed via Ingress + external LoadBalancer (ClusterIP service)
   - gateway: Exposed via internal Layer 4 LoadBalancer

2. **Different Service Types for Different Ports**: Expose some ports via ClusterIP and others via LoadBalancer

3. **Internal and External Access**: Use ClusterIP for internal cluster access and LoadBalancer for external access

### Configuration Options

#### `extraServices` (array of Service objects)
- **Default**: `[]` (empty list)
- **Description**: List of additional service definitions for the same deployment
- **Properties**: Each item in the list supports all the same properties as the primary `service` field

#### `name` (string, optional)
- **Description**: Name suffix for the extra service
- **Example**: If `name: "gateway"` and service name is `httpserver`, the generated service will be `sequencer-httpserver-gateway-service`
- **Note**: If not provided, a default name like `extra-0`, `extra-1` will be used

### Important Notes

- All services (primary + extra) share the same pod selector (labels), so they all target the same pods
- The primary service is used for Ingress and BackendConfig references
- Each extra service can have its own ports, type, annotations, and labels
- Service names are auto-generated: `sequencer-{service-name}-service` (primary) and `sequencer-{service-name}-{extra-name}-service` (extra)

## Generated Kubernetes Resource

The configuration above generates a Service resource like this:

```yaml
apiVersion: v1
kind: Service
metadata:
  name: sequencer-node-service
  namespace: default
  labels:
    app: sequencer
    service: sequencer-node
  annotations:
    service.beta.kubernetes.io/aws-load-balancer-type: nlb
    service.beta.kubernetes.io/aws-load-balancer-scheme: internet-facing
spec:
  type: LoadBalancer
  ports:
    - name: http
      port: 80
      targetPort: 8080
      protocol: TCP
    - name: https
      port: 443
      targetPort: 8443
      protocol: TCP
  selector:
    app: sequencer
    service: sequencer-node
  externalTrafficPolicy: Local
  sessionAffinity: ClientIP
  sessionAffinityConfig:
    clientIP:
      timeoutSeconds: 3600
```

## Best Practices

1. **Service Type**: Choose appropriate service type for your use case
2. **Port Naming**: Use descriptive names for ports
3. **Load Balancer**: Use cloud provider annotations for advanced features
4. **Session Affinity**: Use ClientIP affinity for stateful applications
5. **External Traffic Policy**: Use "Local" for better performance with LoadBalancer
6. **Labels**: Use consistent labeling for better resource management
7. **Annotations**: Use cloud provider annotations for advanced load balancer features
8. **Multiple Services**: Use `extraServices` when you need to expose different ports via different service types (e.g., ClusterIP for Ingress, LoadBalancer for internal access)
9. **Service Naming**: Use descriptive `name` fields in `extraServices` for better service identification
