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

        # Merge common and service podMonitoring configs (service overrides common)
        pod_monitoring_config = self._get_merged_pod_monitoring_config()

        if not pod_monitoring_config or not pod_monitoring_config.enabled:
            return

        self.podmonitoring = self._create_pod_monitoring(pod_monitoring_config)

    def _get_merged_pod_monitoring_config(self):
        """Merge common and service podMonitoring configs.

        Service config takes precedence over common config.
        If service has podMonitoring with enabled=False and no other meaningful config,
        treat it as unset and use common config instead.
        Returns None if neither is configured.
        """
        common_pm = self.common_config.podMonitoring if self.common_config else None
        service_pm = self.service_config.podMonitoring

        # Check if service_pm is essentially a default/empty config
        # (enabled=False, no name, empty annotations/labels, default spec)
        def is_default_pod_monitoring(pm) -> bool:
            """Check if podMonitoring is just a default/empty configuration."""
            if pm is None:
                return True
            if pm.enabled is False:
                # Check if it's essentially empty (no custom name, empty annotations/labels)
                if (
                    pm.name is None
                    and not pm.annotations
                    and not pm.labels
                    and pm.spec
                    and not pm.spec.selector.matchLabels
                    and not pm.spec.selector.matchExpressions
                    and pm.spec.filterRunning is None
                    and pm.spec.limits is None
                    and pm.spec.targetLabels is None
                ):
                    return True
            return False

        # If service has a meaningful podMonitoring config, use it (may merge with common)
        if service_pm is not None and not is_default_pod_monitoring(service_pm):
            # If service has it but common also has it, merge them (service overrides)
            if common_pm:
                # Merge: start with common, then overlay service
                # Use model_dump with mode='python' to get proper dict representation
                merged_dict = common_pm.model_dump(mode="python", exclude_none=True)
                service_dict = service_pm.model_dump(
                    mode="python", exclude_unset=True, exclude_none=True
                )
                # Deep merge the dictionaries recursively
                from copy import deepcopy

                merged = deepcopy(merged_dict)

                def deep_merge(base: dict, overlay: dict) -> dict:
                    """Recursively merge overlay into base."""
                    result = deepcopy(base)
                    for key, value in overlay.items():
                        if (
                            key in result
                            and isinstance(result[key], dict)
                            and isinstance(value, dict)
                        ):
                            result[key] = deep_merge(result[key], value)
                        else:
                            result[key] = value
                    return result

                merged = deep_merge(merged, service_dict)
                from src.config.schema import PodMonitoring

                return PodMonitoring.model_validate(merged)
            return service_pm

        # If service doesn't have podMonitoring or has only default/empty config,
        # use common config if available
        if common_pm:
            return common_pm

        return None

    def _create_pod_monitoring(self, pod_monitoring_config) -> PodMonitoring:
        """Create PodMonitoring resource."""

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
