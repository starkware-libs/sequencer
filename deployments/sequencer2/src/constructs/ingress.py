from constructs import Construct
from imports import k8s

from src.constructs.base import BaseConstruct


class IngressConstruct(BaseConstruct):
    def __init__(
        self,
        scope: Construct,
        id: str,
        common_config,
        service_config,
        labels,
        namespace,
        monitoring_endpoint_port,
    ):
        super().__init__(
            scope,
            id,
            common_config,
            service_config,
            labels,
            monitoring_endpoint_port,
        )
        self.namespace = namespace

        self.ingress = self._create_ingress()

    def _create_ingress(self) -> k8s.KubeIngress:
        """Create a Kubernetes Ingress resource directly from the config."""
        ingress_config = self.service_config.ingress

        # Get service port
        service_port = 8080
        if self.service_config.service and self.service_config.service.ports:
            service_port = self.service_config.service.ports[0].port

        return k8s.KubeIngress(
            self,
            "ingress",
            metadata=k8s.ObjectMeta(
                name=f"{self.service_config.name}-ingress",
                labels=self.labels,
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
                                            name=f"{self.service_config.name}-service",
                                            port=k8s.ServiceBackendPort(number=service_port),
                                        )
                                    ),
                                )
                            ]
                        ),
                    )
                    for host in ingress_config.hosts
                ],
                tls=[
                    k8s.IngressTls(
                        hosts=tls_config["hosts"],
                        secret_name=tls_config["secretName"],
                    )
                    for tls_config in ingress_config.tls
                ],
            ),
        )
