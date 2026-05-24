import json
import os

import _jsonnet
from imports import k8s
from src.config.loaders import ConfigValidationError
from src.constructs.base import BaseConstruct

_JSONNET_SERVICE_KEY_MAP = {
    "sierracompiler": "sierra_compiler",
}


def _jsonnet_service_key(service_name: str) -> str:
    return _JSONNET_SERVICE_KEY_MAP.get(service_name, service_name)


class ConfigMapConstruct(BaseConstruct):
    def __init__(
        self,
        scope,
        id: str,
        service_config,
        labels,
        monitoring_endpoint_port,
        layout: str,
        overlays: list[str],
    ):
        super().__init__(
            scope,
            id,
            service_config,
            labels,
            monitoring_endpoint_port,
        )

        self.layout = layout
        self.overlays = overlays
        self.config_map = self._get_config_map()

    def _parse_env_and_node(self) -> tuple[str | None, str | None]:
        for overlay in self.overlays:
            parts = overlay.split(".")
            if len(parts) >= 3:
                return parts[1], parts[2]
        return None, None

    def _get_config_map(self) -> k8s.KubeConfigMap | None:
        if not self.service_config.config:
            return None

        env_name, node_name = self._parse_env_and_node()
        if not env_name or not node_name:
            raise ConfigValidationError(
                f"Cannot determine environment and node name from overlays: {self.overlays}. "
                f"Expected overlay format: '<layout>.<env>.<node>'"
            )

        service_key = _jsonnet_service_key(self.service_config.name)
        cdk8s_dir = os.path.normpath(
            os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../")
        )
        env_jsonnet_path = os.path.join(
            cdk8s_dir, "configs", "environments", f"{env_name}.jsonnet"
        )

        if not os.path.exists(env_jsonnet_path):
            raise ConfigValidationError(
                f"No environment Jsonnet file found for environment '{env_name}'. "
                f"Expected: {env_jsonnet_path}"
            )

        env_output = json.loads(_jsonnet.evaluate_file(env_jsonnet_path))

        if node_name not in env_output:
            raise ConfigValidationError(
                f"Node '{node_name}' not found in environment '{env_name}'. "
                f"Available nodes: {sorted(env_output.keys())}"
            )

        if service_key not in env_output[node_name]:
            raise ConfigValidationError(
                f"Service '{service_key}' not found for node '{node_name}' in "
                f"environment '{env_name}'. "
                f"Available services: {sorted(env_output[node_name].keys())}"
            )

        node_config: dict = dict(env_output[node_name][service_key])

        sequencer_config = (
            self.service_config.config.sequencerConfig
            if self.service_config.config.sequencerConfig
            else {}
        )
        if sequencer_config:
            unknown_keys = sorted(k for k in sequencer_config if k not in node_config)
            if unknown_keys:
                raise ConfigValidationError(
                    f"Keys in sequencerConfig not found in config for service "
                    f"'{self.service_config.name}': {unknown_keys}"
                )
            node_config.update(sequencer_config)

        node_config = dict(sorted(node_config.items()))
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
