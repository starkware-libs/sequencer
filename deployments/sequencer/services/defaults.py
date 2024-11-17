import dataclasses

from typing import Sequence, Optional, List
from services.objects import *
from config.sequencer import *


@dataclasses.dataclass
class ServiceDefaults:
    image: Optional[str] | None = None
    replicas: Optional[int] = 1
    service_type: Optional[ServiceType] | None = None
    port_mappings: Optional[Sequence[PortMapping]] | None = None
    health_check: Optional[HealthCheck] | None = None
    pvc: Optional[PersistentVolumeClaim] | None = None
    ingress: Optional[Ingress] | None = None
    config: Optional[Config] | None = None
    args: Optional[List[str]] | None = None


sequencer = ServiceDefaults(
    replicas=1,
    config=SequencerDevConfig(mount_path="/app/config"),
    service_type=ServiceType.CLUSTER_IP,
    args=["--config_file", "/app/config/sequencer/config.json"],
    port_mappings=[
        PortMapping(name="http", port=80, container_port=8080),
        PortMapping(name="rpc", port=8081, container_port=8081),
        PortMapping(name="monitoring", port=8082, container_port=8082)
    ],
    health_check=HealthCheck(
        startup_probe=Probe(port=8082, path="/monitoring/nodeVersion", period_seconds=10, failure_threshold=10, timeout_seconds=5),
        readiness_probe=Probe(port=8082, path="/monitoring/ready", period_seconds=10, failure_threshold=5, timeout_seconds=5),
        liveness_probe=Probe(port=8082, path="/monitoring/alive", period_seconds=10, failure_threshold=5, timeout_seconds=5)
    ),
    pvc=PersistentVolumeClaim(
        access_modes=["ReadWriteOnce"],
        storage_class_name="premium-rwo",
        volume_mode="Filesystem",
        storage="256Gi",
        mount_path="/data",
        read_only=False
    ),
    ingress=Ingress(
        annotations={
            "kubernetes.io/tls-acme": "true",
            "cert-manager.io/common-name": "sequencer.gcp-integration.sw-dev.io",
            "cert-manager.io/issue-temporary-certificate": "true",
            "cert-manager.io/issuer": "letsencrypt-prod",
            "acme.cert-manager.io/http01-edit-in-place": "true"
        },
        class_name="gce",
        rules=[
            IngressRule(
                host="sequencer.gcp-integration.sw-dev.io",
                paths=[
                    IngressRuleHttpPath(
                        path="/monitoring/",
                        path_type="Prefix",
                        backend_service_name="sequencer-node-service",
                        backend_service_port_name="monitoring",
                        backend_service_port_number=8082
                    )
                ]
            )
        ],
        tls=[
            IngressTls(
                hosts=[
                    "sequencer.gcp-integration.sw-dev.io"
                ],
                secret_name="sequencer-tls"
            )
        ]
    )
)


