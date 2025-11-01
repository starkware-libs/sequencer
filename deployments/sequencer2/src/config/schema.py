from typing import Any, Dict, List, Optional

from pydantic import BaseModel, ConfigDict, Field, PrivateAttr

StrDict = Dict[str, str]
AnyDict = Dict[str, Any]


class StrictBaseModel(BaseModel):
    model_config = ConfigDict(
        extra="forbid",
        validate_assignment=True,  # re-validate on attribute updates
        arbitrary_types_allowed=False,
        populate_by_name=True,  # Allow both field name and alias
    )


class Image(StrictBaseModel):
    repository: str
    tag: str
    digest: Optional[str] = None
    imagePullPolicy: Optional[str] = None


class UpdateStrategy(StrictBaseModel):
    type: str = "RollingUpdate"


class StatefulSet(StrictBaseModel):
    enabled: Optional[bool] = None
    annotations: StrDict = Field(default_factory=dict)
    labels: StrDict = Field(default_factory=dict)
    podManagementPolicy: Optional[str] = None
    updateStrategy: Optional[UpdateStrategy] = None


class Rbac(StrictBaseModel):
    create: Optional[bool] = None


class ServiceAccount(StrictBaseModel):
    enabled: bool = True
    name: Optional[str] = None
    annotations: StrDict = Field(default_factory=dict)
    labels: StrDict = Field(default_factory=dict)
    # Advanced options
    automountServiceAccountToken: Optional[bool] = None
    imagePullSecrets: List[str] = Field(default_factory=list)
    secrets: List[AnyDict] = Field(default_factory=list)


class SecurityContext(StrictBaseModel):
    runAsUser: Optional[int] = None
    runAsNonRoot: Optional[bool] = None
    runAsGroup: Optional[int] = None
    fsGroup: Optional[int] = None


class ResourcesRequests(StrictBaseModel):
    cpu: Optional[Any] = None
    memory: Optional[str] = None


class ResourcesLimits(StrictBaseModel):
    cpu: Optional[Any] = None
    memory: Optional[str] = None


class Resources(StrictBaseModel):
    requests: Optional[ResourcesRequests] = None
    limits: Optional[ResourcesLimits] = None


class ServicePort(StrictBaseModel):
    name: Optional[str] = None
    port: Optional[int] = None
    targetPort: Optional[int] = None
    protocol: Optional[str] = None


class Service(StrictBaseModel):
    enabled: Optional[bool] = None
    type: Optional[str] = None
    servicePort: Optional[int] = None
    annotations: StrDict = Field(default_factory=dict)
    labels: StrDict = Field(default_factory=dict)
    clusterIP: Optional[str] = None
    externalIPs: List[str] = Field(default_factory=list)
    loadBalancerIP: Optional[str] = None
    loadBalancerSourceRanges: List[str] = Field(default_factory=list)
    sessionAffinity: Optional[str] = None
    ports: List[ServicePort] = Field(default_factory=list)


class Ingress(StrictBaseModel):
    enabled: Optional[bool] = None
    ingressClassName: Optional[str] = None
    annotations: StrDict = Field(default_factory=dict)
    extraLabels: StrDict = Field(default_factory=dict)
    hosts: List[str] = Field(default_factory=list)
    path: Optional[str] = None
    pathType: Optional[str] = None
    extraPaths: List[AnyDict] = Field(default_factory=list)
    tls: List[AnyDict] = Field(default_factory=list)
    # Additional fields for more complex ingress configurations
    internal: Optional[bool] = None
    alternative_names: List[str] = Field(default_factory=list)
    rules: List[AnyDict] = Field(default_factory=list)
    cloud_armor_policy_name: Optional[str] = None


class PodDisruptionBudget(StrictBaseModel):
    enabled: bool = False
    name: Optional[str] = None
    annotations: StrDict = Field(default_factory=dict)
    labels: StrDict = Field(default_factory=dict)
    selector: AnyDict = Field(default_factory=dict)  # matchLabels and/or matchExpressions
    minAvailable: Optional[int | str] = None  # int or string like "50%"
    maxUnavailable: Optional[int | str] = None  # int or string like "50%"
    unhealthyPodEvictionPolicy: Optional[str] = None  # IfHealthyBudget, AlwaysAllow


class PersistentVolume(StrictBaseModel):
    enabled: Optional[bool] = None
    volumeMode: Optional[str] = None
    accessModes: List[str] = Field(default_factory=list)
    labels: StrDict = Field(default_factory=dict)
    annotations: StrDict = Field(default_factory=dict)
    existingClaim: Optional[str] = None
    mountPath: Optional[str] = None
    size: Optional[str] = None
    storageClass: Optional[str] = None
    volumeName: Optional[str] = None


class Probe(StrictBaseModel):
    enabled: Optional[bool] = None
    probeScheme: Optional[str] = None
    path: Optional[str] = None
    periodSeconds: Optional[int] = None
    failureThreshold: Optional[int] = None
    successThreshold: Optional[int] = None
    timeoutSeconds: Optional[int] = None


class HPA(StrictBaseModel):
    enabled: bool = False
    minReplicas: int = 1
    maxReplicas: int = 100
    targetCPUUtilizationPercentage: Optional[int] = None
    targetMemoryUtilizationPercentage: Optional[int] = None
    # Additional flexible options
    behavior: Optional[AnyDict] = None  # Custom scaling behavior
    metrics: List[AnyDict] = Field(default_factory=list)  # Custom metrics
    scaleUpStabilizationWindowSeconds: Optional[int] = None
    scaleDownStabilizationWindowSeconds: Optional[int] = None
    scaleUpPolicies: List[AnyDict] = Field(default_factory=list)
    scaleDownPolicies: List[AnyDict] = Field(default_factory=list)


class ExternalSecretData(StrictBaseModel):
    secretKey: str
    remoteKey: str
    property: Optional[str] = None  # For JSON property extraction


class ExternalSecretStore(StrictBaseModel):
    name: str
    kind: str = "ClusterSecretStore"  # ClusterSecretStore or SecretStore
    provider: str = "gcp"  # gcp, aws, azure, vault, etc.


class ExternalSecret(StrictBaseModel):
    enabled: bool = False
    secretStore: ExternalSecretStore = Field(
        default_factory=lambda: ExternalSecretStore(name="external-secrets-project")
    )
    refreshInterval: str = "1m"
    targetName: Optional[str] = None  # Custom target secret name
    data: List[ExternalSecretData] = Field(default_factory=list)
    mountPath: Optional[str] = None  # Where to mount the external secret (default: /app/secrets)
    # Advanced options
    template: Optional[AnyDict] = None  # Custom template for secret generation
    metadata: Optional[AnyDict] = None  # Custom metadata for the secret
    deletionPolicy: str = "Retain"  # Retain, Delete, Merge


class HealthCheck(StrictBaseModel):
    port: Optional[int] = None
    requestPath: Optional[str] = None
    checkIntervalSeconds: Optional[int] = None
    timeoutSeconds: Optional[int] = None
    healthyThreshold: Optional[int] = None
    unhealthyThreshold: Optional[int] = None


class BackendConfig(StrictBaseModel):
    enabled: Optional[bool] = None
    customRequestHeaders: List[str] = Field(default_factory=list)
    connectionDrainingTimeoutSeconds: Optional[int] = None
    securityPolicy: Optional[str] = None
    timeOutSeconds: Optional[int] = None
    healthCheck: Optional[HealthCheck] = None


class Secret(StrictBaseModel):
    enabled: bool = False
    name: Optional[str] = None
    type: str = "Opaque"
    data: StrDict = Field(default_factory=dict)
    stringData: StrDict = Field(default_factory=dict)
    annotations: StrDict = Field(default_factory=dict)
    labels: StrDict = Field(default_factory=dict)
    immutable: Optional[bool] = None
    mountPath: Optional[str] = None  # Where to mount the secret (default: /app/secrets)


class CommonConfig(StrictBaseModel):
    image: Image = Image(repository="ghcr.io/starkware-libs/sequencer", tag="dev")
    imagePullSecrets: List[str] = Field(default_factory=list)
    commonMetaLabels: StrDict = Field(default_factory=dict)


class PodMonitoringEndpoint(StrictBaseModel):
    port: int | str  # Port name or number (required)
    path: Optional[str] = "/metrics"  # HTTP path to scrape (default: /metrics)
    interval: Optional[str] = "10s"  # Scrape interval (Prometheus duration format)
    timeout: Optional[str] = None  # Scrape timeout (must be < interval)
    scheme: Optional[str] = None  # Protocol scheme (http/https)
    params: Optional[AnyDict] = None  # HTTP GET params
    proxyUrl: Optional[str] = None  # HTTP proxy URL
    # Advanced options
    metricRelabeling: List[AnyDict] = Field(default_factory=list)  # Metric relabeling rules
    authorization: Optional[AnyDict] = None  # HTTP authorization credentials
    basicAuth: Optional[AnyDict] = None  # HTTP basic auth
    oauth2: Optional[AnyDict] = None  # OAuth2 credentials
    tls: Optional[AnyDict] = None  # TLS configuration


class PodMonitoringSelector(StrictBaseModel):
    matchLabels: StrDict = Field(default_factory=dict)
    matchExpressions: List[AnyDict] = Field(default_factory=list)


class PodMonitoringLimits(StrictBaseModel):
    samples: Optional[int] = None  # Max samples per scrape
    labels: Optional[int] = None  # Max labels per sample
    labelNameLength: Optional[int] = None  # Max label name length
    labelValueLength: Optional[int] = None  # Max label value length


class PodMonitoringTargetLabels(StrictBaseModel):
    metadata: List[str] = Field(default_factory=list)  # pod, container, node, namespace
    fromPod: List[AnyDict] = Field(default_factory=list)  # Label mappings from pod labels


class PodMonitoringSpec(StrictBaseModel):
    endpoints: List[PodMonitoringEndpoint]  # Required: list of endpoints to scrape
    selector: PodMonitoringSelector  # Required: pod selector
    filterRunning: Optional[bool] = None  # Filter out Failed/Succeeded pods
    limits: Optional[PodMonitoringLimits] = None  # Scrape limits
    targetLabels: Optional[PodMonitoringTargetLabels] = None  # Labels to add to targets


class PodMonitoring(StrictBaseModel):
    enabled: bool = False
    name: Optional[str] = None
    annotations: StrDict = Field(default_factory=dict)
    labels: StrDict = Field(default_factory=dict)
    spec: PodMonitoringSpec


class NetworkPolicy(StrictBaseModel):
    enabled: bool = False
    name: Optional[str] = None
    annotations: StrDict = Field(default_factory=dict)
    labels: StrDict = Field(default_factory=dict)
    # podSelector: LabelSelector - uses matchLabels and/or matchExpressions
    podSelector: AnyDict = Field(default_factory=dict)  # matchLabels and/or matchExpressions
    policyTypes: List[str] = Field(default_factory=list)  # ["Ingress", "Egress"]
    ingress: List[AnyDict] = Field(default_factory=list)  # NetworkPolicyIngressRule
    egress: List[AnyDict] = Field(default_factory=list)  # NetworkPolicyEgressRule


class PriorityClass(StrictBaseModel):
    enabled: bool = False
    name: Optional[str] = None
    annotations: StrDict = Field(default_factory=dict)
    labels: StrDict = Field(default_factory=dict)
    value: int  # Required: priority value (higher = more important)
    globalDefault: bool = False  # Whether this is the default PriorityClass
    description: Optional[str] = None  # Description of the PriorityClass
    preemptionPolicy: Optional[str] = None  # "Never" or "PreemptLowerPriority"


class ServiceConfig(StrictBaseModel):
    _source: str | None = PrivateAttr(default=None)
    name: str
    configPaths: List[str] = Field(default_factory=list)
    replicas: int = 1
    statefulSet: Optional[StatefulSet] = None
    rbac: Optional[Rbac] = None
    serviceAccount: Optional[ServiceAccount] = None
    terminationGracePeriodSeconds: Optional[int] = None
    command: List[str] = Field(default_factory=list)
    args: List[str] = Field(default_factory=list)
    priorityClassName: Optional[str] = None
    env: List[AnyDict] = Field(default_factory=list)
    securityContext: Optional[SecurityContext] = None
    resources: Optional[Resources] = None
    service: Optional[Service] = None
    ingress: Optional[Ingress] = None
    updateStrategy: UpdateStrategy = Field(default_factory=UpdateStrategy)
    tolerations: List[AnyDict] = Field(default_factory=list)
    nodeSelector: StrDict = Field(default_factory=dict)
    affinity: AnyDict = Field(default_factory=dict)
    podAntiAffinity: AnyDict = Field(default_factory=dict)
    topologySpreadConstraints: List[AnyDict] = Field(default_factory=list)
    podDisruptionBudget: Optional[PodDisruptionBudget] = None
    persistentVolume: Optional[PersistentVolume] = None
    podAnnotations: StrDict = Field(default_factory=dict)
    podLabels: StrDict = Field(default_factory=dict)
    configMapAnnotations: StrDict = Field(default_factory=dict)
    deploymentAnnotations: StrDict = Field(default_factory=dict)
    startupProbe: Optional[Probe] = None
    readinessProbe: Optional[Probe] = None
    livenessProbe: Optional[Probe] = None
    hpa: Optional[HPA] = None
    dnsPolicy: Optional[str] = None
    backendConfig: Optional[BackendConfig] = Field(
        default=None, alias="gcpBackendConfig"
    )  # Accepts both backendConfig and gcpBackendConfig in YAML
    externalSecret: Optional[ExternalSecret] = None
    secret: Optional[Secret] = None
    podMonitoring: Optional[PodMonitoring] = Field(
        default=None, alias="gcpPodMonitoring"
    )  # Accepts both podMonitoring and gcpPodMonitoring in YAML
    networkPolicy: Optional[NetworkPolicy] = None
    priorityClass: Optional[PriorityClass] = None


class DeploymentConfig(StrictBaseModel):
    common: CommonConfig = Field(default_factory=CommonConfig)
    services: List[ServiceConfig] = Field(default_factory=list)
