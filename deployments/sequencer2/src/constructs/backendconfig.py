from cdk8s import ApiObjectMetadata
from imports.com.google.cloud import (
    BackendConfig,
    BackendConfigSpec,
    BackendConfigSpecConnectionDraining,
    BackendConfigSpecCustomRequestHeaders,
    BackendConfigSpecHealthCheck,
    BackendConfigSpecSecurityPolicy,
)

from src.constructs.base import BaseConstruct


class BackendConfigConstruct(BaseConstruct):
    def __init__(
        self,
        scope,
        id: str,
        common_config,
        service_config,
        labels,
        monitoring_endpoint_port,
    ):
        super().__init__(scope, id, common_config, service_config, labels, monitoring_endpoint_port)

        self.backend_config = self._get_backend_config()

    def _get_backend_config(self) -> BackendConfig:
        return BackendConfig(
            self,
            "backend-config",
            metadata=ApiObjectMetadata(
                labels=self.labels,
            ),
            spec=BackendConfigSpec(
                custom_request_headers=BackendConfigSpecCustomRequestHeaders(
                    headers=self.service_config.backendConfig.customRequestHeaders
                ),
                connection_draining=BackendConfigSpecConnectionDraining(
                    draining_timeout_sec=self.service_config.backendConfig.connectionDrainingTimeoutSeconds,
                ),
                security_policy=(
                    BackendConfigSpecSecurityPolicy(
                        name=self.service_config.backendConfig.securityPolicy
                    )
                ),
                timeout_sec=self.service_config.backendConfig.timeOutSeconds,
                health_check=BackendConfigSpecHealthCheck(
                    port=self.monitoring_endpoint_port,
                    request_path=self.service_config.backendConfig.healthCheck.requestPath,
                    check_interval_sec=self.service_config.backendConfig.healthCheck.checkIntervalSeconds,
                    timeout_sec=self.service_config.backendConfig.healthCheck.timeoutSeconds,
                    healthy_threshold=self.service_config.backendConfig.healthCheck.healthyThreshold,
                    unhealthy_threshold=self.service_config.backendConfig.healthCheck.unhealthyThreshold,
                ),
            ),
        )
