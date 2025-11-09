import typing

from imports import k8s

from src.constructs.base import BaseConstruct


class ServiceConstruct(BaseConstruct):
    def __init__(
        self,
        scope,
        id: str,
        common_config,
        service_config,
        labels,
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

    def _get_service_type(self, service_spec) -> str:
        """Get Kubernetes service type, normalized to standard values."""
        svc_type = service_spec.type or "ClusterIP"
        # Normalize to Kubernetes standard service types
        svc_type_lower = svc_type.lower()
        if svc_type_lower in ["loadbalancer", "lb"]:
            return "LoadBalancer"
        elif svc_type_lower in ["nodeport", "np"]:
            return "NodePort"
        elif svc_type_lower in ["clusterip", "cluster", ""]:
            return "ClusterIP"
        else:
            # If already correctly formatted, return as-is
            if svc_type in ["LoadBalancer", "NodePort", "ClusterIP"]:
                return svc_type
            raise ValueError(
                f"Unknown service type: {svc_type}. Valid types: LoadBalancer, NodePort, ClusterIP"
            )

    def _get_service_ports(self, service_spec) -> typing.List[k8s.ServicePort]:
        """Convert Pydantic ports list into Kubernetes ServicePort objects with sane defaults.

        Merges common service ports with service-specific ports. Service-specific ports
        take precedence if there's a name conflict.
        """
        ports: list[k8s.ServicePort] = []

        # Start with common service ports (if they exist)
        common_ports_dict = {}
        if self.common_config.service and self.common_config.service.ports:
            for p in self.common_config.service.ports:
                if p.name:
                    common_ports_dict[p.name] = p

        # Add service-specific ports (override common ports with same name)
        service_ports_dict = {}
        for p in service_spec.ports:
            if p.name:
                service_ports_dict[p.name] = p

        # Merge: common first, then service-specific (service-specific overrides)
        merged_ports_dict = {**common_ports_dict, **service_ports_dict}

        # Convert to list, preserving service-specific port order, then adding remaining common ports
        # This ensures service-specific ports appear first
        processed_names = set()
        for p in service_spec.ports:
            processed_names.add(p.name if p.name else None)

        # Add service-specific ports first (preserve their order)
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
            processed_names.add(p.name if p.name else None)

        # Add remaining common ports that weren't overridden by service-specific ports
        if self.common_config.service and self.common_config.service.ports:
            for p in self.common_config.service.ports:
                if p.name and p.name not in processed_names:
                    # Default targetPort to port if not provided
                    target = p.targetPort if getattr(p, "targetPort", None) is not None else p.port

                    # Build IntOrString for targetPort
                    if isinstance(target, (int, float)):
                        target_ios = k8s.IntOrString.from_number(int(target))
                    else:
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
