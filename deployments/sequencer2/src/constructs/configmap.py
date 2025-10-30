import json

from constructs import Construct
from imports import k8s

from src.config.loaders import NodeConfigLoader
from src.constructs.base import BaseConstruct


class ConfigMapConstruct(BaseConstruct):
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

        self.config_map = self._get_config_map()

    def _get_config_map(self) -> k8s.KubeConfigMap:
        # Try to load JSON configs using NodeConfigLoader if config_paths are provided
        if self.service_config.configPaths:
            try:
                node_config_loader = NodeConfigLoader(
                    config_paths=self.service_config.configPaths,
                )
                node_config = node_config_loader.load()
                config_data = json.dumps(node_config, indent=2)
            except (ValueError, FileNotFoundError):
                # Fallback to common_config if JSON files don't exist
                config_data = json.dumps(self.common_config, indent=2)
        else:
            # Fallback to common_config if no config_paths
            config_data = json.dumps(self.common_config, indent=2)

        return k8s.KubeConfigMap(
            self,
            "configmap",
            metadata=k8s.ObjectMeta(name=f"{self.service_config.name}-config"),
            data=dict(config=config_data),
        )
