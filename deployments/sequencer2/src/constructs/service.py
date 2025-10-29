import typing

from constructs import Construct
from imports import k8s

from src.config import constants as const


class ServiceConstruct(Construct):
    def __init__(
        self,
        scope: Construct,
        id: str,
        service_config,
        labels: dict,
        node_config: dict,
    ):
        super().__init__(scope, id)

        self.service_config = service_config
        self.labels = labels
        self.node_config = node_config

        self.service = self._create_service()

    def _create_service(self) -> k8s.KubeService:
        service_spec = self.service_config.service
        return k8s.KubeService(
            self,
            "service",
            metadata=k8s.ObjectMeta(
                labels=self.labels,
                annotations=self._get_service_annotations(service_spec),
            ),
            spec=k8s.ServiceSpec(
                type=self._get_service_type(service_spec),
                ports=self._get_service_ports(service_spec),
                selector=self.labels,
                cluster_ip=service_spec.clusterIP or None,
                external_i_ps=service_spec.externalIPs or None,
                load_balancer_ip=service_spec.loadBalancerIP or None,
                load_balancer_source_ranges=service_spec.loadBalancerSourceRanges or None,
                session_affinity=service_spec.sessionAffinity or "None",
            ),
        )

    def _get_service_annotations(self, service_spec) -> typing.Dict[str, str]:
        """Merge custom annotations and GKE-specific internal/external hints."""
        annotations = dict(service_spec.annotations or {})
        svc_type = service_spec.type

        # Example: automatically annotate internal load balancers for GKE
        if svc_type == "LoadBalancer" and getattr(service_spec, "internal", False):
            annotations.update(
                {
                    "cloud.google.com/load-balancer-type": "Internal",
                    "networking.gke.io/internal-load-balancer-allow-global-access": "true",
                }
            )

        # Add external DNS hostname if defined
        external_dns = getattr(service_spec, "external_dns_name", None)
        if external_dns:
            annotations["external-dns.alpha.kubernetes.io/hostname"] = external_dns

        return annotations

    def _get_service_type(self, service_spec) -> const.K8SServiceType:
        svc_type = (service_spec.type or "ClusterIP").capitalize()
        if svc_type == "Loadbalancer":
            return const.K8SServiceType.LOAD_BALANCER
        elif svc_type == "Nodeport":
            return const.K8SServiceType.NODE_PORT
        elif svc_type == "Clusterip":
            return const.K8SServiceType.CLUSTER_IP
        else:
            raise ValueError(f"Unknown service type: {svc_type}")

    def _get_service_ports(self, service_spec) -> typing.List[k8s.ServicePort]:
        """Convert Pydantic ports list into Kubernetes ServicePort objects with sane defaults."""
        ports: list[k8s.ServicePort] = []

        for p in service_spec.ports:
            # Validate required "port"
            if p.port is None:
                raise ValueError(
                    f"Service port entry is missing 'port' (service: {getattr(self.service_config, 'name', '<unknown>')})"
                )

            # Default targetPort to port if not provided
            target = p.targetPort if getattr(p, "targetPort", None) is not None else p.port

            # Build IntOrString for targetPort
            if isinstance(target, (int, float)):
                target_ios = k8s.IntOrString.from_number(int(target))
            else:
                # allow named port like "http" or "monitoring"
                target_ios = k8s.IntOrString.from_string(str(target))

            ports.append(
                k8s.ServicePort(
                    name=p.name,
                    port=int(p.port),
                    target_port=target_ios,
                    protocol=(p.protocol or "TCP"),
                )
            )

        return ports
