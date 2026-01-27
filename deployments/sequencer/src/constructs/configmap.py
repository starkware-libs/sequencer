import json

from imports import k8s
from src.config.loaders import NodeConfigLoader
from src.constructs.base import BaseConstruct


class ConfigMapConstruct(BaseConstruct):
    def __init__(
        self,
        scope,
        id: str,
        service_config,
        labels,
        monitoring_endpoint_port,
        layout: str,
        overlay: str | None,
    ):
        super().__init__(
            scope,
            id,
            service_config,
            labels,
            monitoring_endpoint_port,
        )

        self.layout = layout
        self.overlay = overlay
        self.config_map = self._get_config_map()

    def _get_config_map(self) -> k8s.KubeConfigMap:
        # config is mandatory
        if not self.service_config.config:
            raise ValueError(
                f"config is required for service '{self.service_config.name}' but was not provided"
            )
        if not self.service_config.config.configList:
            raise ValueError(
                f"config.configList is required for service '{self.service_config.name}' but was not provided"
            )

        # Load JSON configs using NodeConfigLoader
        node_config_loader = NodeConfigLoader(
            config_list_json_path=self.service_config.config.configList,
        )
        node_config = node_config_loader.load()

        # sequencerConfig is now already merged from common into service_config
        merged_sequencer_config = (
            self.service_config.config.sequencerConfig
            if self.service_config.config and self.service_config.config.sequencerConfig
            else {}
        )

        # Apply merged overrides (includes validation for both unused keys and remaining placeholders)
        if merged_sequencer_config:
            node_config = NodeConfigLoader.apply_sequencer_overrides(
                node_config,
                merged_sequencer_config,
                service_name=self.service_config.name,
                config_list_path=self.service_config.config.configList,
                layout=self.layout,
                overlay=self.overlay,
            )
        else:
            # If no sequencer config overrides, still validate for remaining placeholders
            NodeConfigLoader.validate_no_remaining_placeholders(
                node_config,
                config_list_path=self.service_config.config.configList,
                layout=self.layout,
                overlay=self.overlay,
            )

        config_data = json.dumps(node_config, indent=2)

        return k8s.KubeConfigMap(
            self,
            "configmap",
            metadata=k8s.ObjectMeta(
                name=f"sequencer-{self.service_config.name}-config",
                labels=self.labels,
            ),
            data=dict(config=config_data),
        )  # Key is "config" to match node/ format, mounted as /config/sequencer/presets/config
