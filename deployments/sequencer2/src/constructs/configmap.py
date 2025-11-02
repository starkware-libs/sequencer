import json

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
        # config is mandatory
        if not self.service_config.config or not self.service_config.config.configPaths:
            raise ValueError(
                f"config.configPaths is required for service '{self.service_config.name}' but was not provided"
            )

        # Load JSON configs using NodeConfigLoader
        node_config_loader = NodeConfigLoader(
            config_paths=self.service_config.config.configPaths,
        )
        node_config = node_config_loader.load()
        config_data = json.dumps(node_config, indent=2)

        return k8s.KubeConfigMap(
            self,
            "configmap",
            metadata=k8s.ObjectMeta(name=f"sequencer-{self.service_config.name}-config"),
            data=dict(config=config_data),
        )
