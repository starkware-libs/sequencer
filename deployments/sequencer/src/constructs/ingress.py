from constructs import Construct
from imports import k8s
from src.constructs.base import BaseConstruct


class IngressConstruct(BaseConstruct):
    def __init__(
        self,
        scope: Construct,
        id: str,
        service_config,
        labels,
        namespace,
        monitoring_endpoint_port,
    ):
        super().__init__(
            scope,
            id,
            service_config,
            labels,
            monitoring_endpoint_port,
        )
        self.namespace = namespace

        self.ingress = self._create_ingress()

    def _create_ingress(self) -> k8s.KubeIngress:
        """Create a Kubernetes Ingress resource directly from the config."""
        ingress_config = self.service_config.ingress

        # Merge Ingress labels with common labels
        ingress_labels = (
            {**self.labels, **ingress_config.labels} if ingress_config.labels else self.labels
        )

        # Get backend service name (use custom if provided, otherwise default)
        backend_service_name = (
            ingress_config.backendServiceName
            if ingress_config.backendServiceName
            else f"sequencer-{self.service_config.name}-service"
        )

        # Backend port is required when ingress is enabled (validated in schema)
        # This check provides a safety net in case validation somehow didn't catch it
        backend_service_port = ingress_config.backendServicePort
        if backend_service_port is None:
            raise ValueError(
                "backendServicePort is required when ingress is enabled. "
                "Please explicitly set backendServicePort in your ingress configuration."
            )

        return k8s.KubeIngress(
            self,
            "ingress",
            metadata=k8s.ObjectMeta(
                name=f"sequencer-{self.service_config.name}-ingress",
                labels=ingress_labels,
                annotations=ingress_config.annotations,
            ),
            spec=k8s.IngressSpec(
                ingress_class_name=ingress_config.ingressClassName,
                rules=[
                    k8s.IngressRule(
                        host=host,
                        http=k8s.HttpIngressRuleValue(
                            paths=[
                                k8s.HttpIngressPath(
                                    path=ingress_config.path or "/",
                                    path_type=ingress_config.pathType or "Prefix",
                                    backend=k8s.IngressBackend(
                                        service=k8s.IngressServiceBackend(
                                            name=backend_service_name,
                                            port=k8s.ServiceBackendPort(
                                                number=backend_service_port
                                            ),
                                        )
                                    ),
                                )
                            ]
                        ),
                    )
                    for host in ingress_config.hosts
                ],
                tls=(
                    [
                        k8s.IngressTls(
                            hosts=tls_config["hosts"],
                            secret_name=tls_config.get(
                                "secretName", f"sequencer-{self.service_config.name}-tls"
                            ),
                        )
                        for tls_config in ingress_config.tls
                    ]
                    if ingress_config.tls
                    else None
                ),
            ),
        )
