import typing

from cdk8s import ApiObjectMetadata
from constructs import Construct
from imports import k8s
from imports.com.google.cloud import (
    BackendConfig,
    BackendConfigSpec,
    BackendConfigSpecConnectionDraining,
    BackendConfigSpecCustomRequestHeaders,
    BackendConfigSpecHealthCheck,
    BackendConfigSpecSecurityPolicy,
)

from src.config import constants as const


class IngressConstruct(Construct):
    def __init__(
        self,
        scope: Construct,
        id: str,
        service_topology,
        labels,
        namespace,
        monitoring_endpoint_port,
    ):
        super().__init__(scope, id)

        self.service_topology = service_topology
        self.labels = labels
        self.namespace = namespace
        self.monitoring_endpoint_port = monitoring_endpoint_port

        self.ingress = self._get_ingress()
        self.backend_config = self._get_backend_config(
            security_policy_name=self._get_cloud_armor_policy_name()
        )

    def _get_ingress(self) -> k8s.KubeIngress:
        domain = self.service_topology.ingress["domain"]
        self.host = f"{self.node.id}.{self.namespace}.{domain}"
        dns_names = self.host
        rules = [self._get_ingress_rule(self.host)]
        tls = self._get_ingress_tls()

        annotations = {
            "kubernetes.io/tls-acme": "true",
            "external-dns.alpha.kubernetes.io/hostname": self.host,
            "external-dns.alpha.kubernetes.io/ingress-hostname-source": "annotation-only",
            "cert-manager.io/common-name": self.host,
            "cert-manager.io/issue-temporary-certificate": "true",
            "cert-manager.io/issuer": "letsencrypt-prod",
            "acme.cert-manager.io/http01-edit-in-place": "true",
        }

        if self.service_topology.ingress["internal"] == True:
            annotations.clear()
            annotations.update({"kubernetes.io/ingress.class": "gce-internal"})
            tls = None

        elif self.service_topology.ingress.get("alternative_names", []):
            alternative_names = self.service_topology.ingress["alternative_names"]
            for alt_name in alternative_names:
                if alt_name != self.host:
                    dns_names += f",{alt_name}"
                    rules.append(self._get_ingress_rule(alt_name))
            annotations.update({"cert-manager.io/dns-names": dns_names})

        return k8s.KubeIngress(
            self,
            "ingress",
            metadata=k8s.ObjectMeta(
                name=f"{self.node.id}-ingress",
                labels=self.labels,
                annotations=annotations,
            ),
            spec=k8s.IngressSpec(
                tls=tls,
                rules=rules,
            ),
        )

    def _get_ingress_rule(self, host: str) -> k8s.IngressRule:
        paths = []

        for rule in self.service_topology.ingress["rules"]:
            path = rule["path"]
            port = rule["port"]
            backend = rule.get("backend")
            paths.append(self._get_ingress_path(path=path, port=port, backend=backend))

        return k8s.IngressRule(
            host=host,
            http=k8s.HttpIngressRuleValue(
                paths=paths,
            ),
        )

    def _get_ingress_tls(self) -> typing.List[k8s.IngressTls]:
        hosts = [self.host]
        if self.service_topology.ingress.get("alternative_names", []):
            alternative_names = self.service_topology.ingress["alternative_names"]
            for alt_name in alternative_names:
                if alt_name != self.host:
                    hosts.append(alt_name)
        return [k8s.IngressTls(hosts=hosts, secret_name=f"{self.node.id}-tls")]

    def _get_ingress_path(self, path: str, port: int, backend: str = None) -> k8s.HttpIngressPath:
        if backend is None:
            backend = f"{self.node.id}-service"

        return k8s.HttpIngressPath(
            path=path,
            path_type="Prefix",
            backend=k8s.IngressBackend(
                service=k8s.IngressServiceBackend(
                    name=backend,
                    port=k8s.ServiceBackendPort(number=port),
                )
            ),
        )

    def _get_backend_config(self, security_policy_name: typing.Optional[str]) -> BackendConfig:
        return BackendConfig(
            self,
            "backend-config",
            metadata=ApiObjectMetadata(
                name=f"{self.node.id}-backend-config",
                labels=self.labels,
            ),
            spec=BackendConfigSpec(
                custom_request_headers=BackendConfigSpecCustomRequestHeaders(
                    headers=const.BACKEND_CONFIG_HEADERS,
                ),
                connection_draining=BackendConfigSpecConnectionDraining(
                    draining_timeout_sec=const.BACKEND_CONFIG_CONNECTION_DRAINING_SECONDS
                ),
                security_policy=(
                    BackendConfigSpecSecurityPolicy(name=security_policy_name)
                    if security_policy_name
                    else None
                ),
                timeout_sec=const.BACKEND_CONFIG_TIMEOUT_SECONDS,
                health_check=BackendConfigSpecHealthCheck(
                    port=self.monitoring_endpoint_port,
                    request_path=const.MONITORING_METRICS_ENDPOINT,
                    check_interval_sec=const.BACKEND_CONFIG_HEALTH_CHECK_INTERVAL_SECONDS,
                    timeout_sec=const.BACKEND_CONFIG_HEALTH_CHECK_TIMEOUT_SECONDS,
                    healthy_threshold=const.BACKEND_CONFIG_HEALTHY_THRESHOLD,
                    unhealthy_threshold=const.BACKEND_CONFIG_UNHEALTHY_THRESHOLD,
                ),
            ),
        )

    def _get_cloud_armor_policy_name(self) -> typing.Optional[str]:
        if self.service_topology.ingress.get("internal"):
            return None
        return self.service_topology.ingress.get("cloud_armor_policy_name")
