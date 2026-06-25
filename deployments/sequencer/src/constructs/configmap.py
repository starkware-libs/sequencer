import json
from pathlib import Path

from imports import k8s
from src.config.native import build_native_config
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

        node_config = self._build_native_node_config()

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

    def _build_native_node_config(self) -> dict:
        """Produce the nested SequencerNodeConfig for this service by evaluating the leaf overlay's
        self-contained `node.jsonnet` (which carries all applicative config via jsonnet `build()`)."""
        if not self.overlays:
            raise ValueError(
                "--config-format native requires at least one -o/--overlay "
                "(the leaf overlay's node.jsonnet is the config source)."
            )
        base_dir = Path(__file__).resolve().parents[2]
        # The leaf `-o` overlay's dir (its dotted path under configs/overlays) holds node.jsonnet.
        node_file = base_dir.joinpath("configs", "overlays", *self.overlays[-1].split(".")) / "node.jsonnet"
        return build_native_config(self.service_config.name, node_file)
