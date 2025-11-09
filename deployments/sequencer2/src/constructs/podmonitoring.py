from cdk8s import ApiObjectMetadata
from imports.com.googleapis.monitoring import (
    PodMonitoring,
    PodMonitoringSpec,
    PodMonitoringSpecEndpoints,
    PodMonitoringSpecEndpointsPort,
    PodMonitoringSpecSelector,
)

from src.constructs.base import BaseConstruct


class PodMonitoringConstruct(BaseConstruct):
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

        # podMonitoring is now already merged from common into service_config
        if not self.service_config.podMonitoring or not self.service_config.podMonitoring.enabled:
            return

        self.podmonitoring = self._create_pod_monitoring()

    def _create_pod_monitoring(self) -> PodMonitoring:
        """Create PodMonitoring resource."""
        pod_monitoring_config = self.service_config.podMonitoring

        # Merge labels with common labels
        merged_labels = {**self.labels, **pod_monitoring_config.labels}

        # Build selector - use provided selector or default to pod labels
        # This ensures selector stays in sync with pod labels automatically
        selector_match_labels = pod_monitoring_config.spec.selector.matchLabels
        selector_match_expressions = pod_monitoring_config.spec.selector.matchExpressions or []

        # If no matchLabels specified and no matchExpressions, use pod labels
        if not selector_match_labels and not selector_match_expressions:
            selector_match_labels = self.labels

        selector = PodMonitoringSpecSelector(
            match_labels=selector_match_labels,
            match_expressions=selector_match_expressions or None,
        )

        # Build endpoints
        endpoints = []
        for endpoint_config in pod_monitoring_config.spec.endpoints:
            port = endpoint_config.port if endpoint_config.port else self.monitoring_endpoint_port
            endpoint_kwargs = {
                "port": (
                    PodMonitoringSpecEndpointsPort.from_number(port)
                    if isinstance(port, int)
                    else PodMonitoringSpecEndpointsPort.from_string(port)
                ),
                "path": endpoint_config.path,
                "interval": endpoint_config.interval,
                "timeout": endpoint_config.timeout,
                "scheme": endpoint_config.scheme,
                "params": endpoint_config.params,
                "proxy_url": endpoint_config.proxyUrl,
            }
            # Add advanced options if specified
            if endpoint_config.metricRelabeling:
                endpoint_kwargs["metric_relabeling"] = endpoint_config.metricRelabeling
            if endpoint_config.authorization:
                endpoint_kwargs["authorization"] = endpoint_config.authorization
            if endpoint_config.basicAuth:
                endpoint_kwargs["basic_auth"] = endpoint_config.basicAuth
            if endpoint_config.oauth2:
                endpoint_kwargs["oauth2"] = endpoint_config.oauth2
            if endpoint_config.tls:
                endpoint_kwargs["tls"] = endpoint_config.tls

            # Remove None values
            endpoint_kwargs = {k: v for k, v in endpoint_kwargs.items() if v is not None}
            endpoints.append(PodMonitoringSpecEndpoints(**endpoint_kwargs))

        # Build spec
        spec_kwargs = {
            "selector": selector,
            "endpoints": endpoints,
        }
        if pod_monitoring_config.spec.filterRunning is not None:
            spec_kwargs["filter_running"] = pod_monitoring_config.spec.filterRunning

        # Handle limits - convert camelCase to snake_case for CDK8s
        if pod_monitoring_config.spec.limits:
            limits_dict = pod_monitoring_config.spec.limits.model_dump(exclude_none=True)
            if limits_dict:
                # Convert camelCase keys to snake_case for CDK8s
                limits_cdk8s = {}
                for key, value in limits_dict.items():
                    # Convert camelCase to snake_case (labelNameLength -> label_name_length)
                    snake_key = "".join("_" + c.lower() if c.isupper() else c for c in key).lstrip(
                        "_"
                    )
                    limits_cdk8s[snake_key] = value
                spec_kwargs["limits"] = limits_cdk8s

        # Handle targetLabels - convert camelCase to snake_case for CDK8s
        if pod_monitoring_config.spec.targetLabels:
            target_labels_dict = pod_monitoring_config.spec.targetLabels.model_dump(
                exclude_none=True
            )
            if target_labels_dict:
                # Convert camelCase keys to snake_case for CDK8s
                target_labels_cdk8s = {}
                for key, value in target_labels_dict.items():
                    snake_key = "".join("_" + c.lower() if c.isupper() else c for c in key).lstrip(
                        "_"
                    )
                    target_labels_cdk8s[snake_key] = value
                spec_kwargs["target_labels"] = target_labels_cdk8s

        spec = PodMonitoringSpec(**spec_kwargs)

        # Build resource name
        name = (
            pod_monitoring_config.name
            if pod_monitoring_config.name
            else f"sequencer-{self.service_config.name}-pod-monitoring"
        )

        return PodMonitoring(
            self,
            "pod-monitoring",
            metadata=ApiObjectMetadata(
                name=name,
                labels=merged_labels,
                annotations=pod_monitoring_config.annotations,
            ),
            spec=spec,
        )
