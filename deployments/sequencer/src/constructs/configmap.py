import json
import os
import subprocess
import tempfile

from imports import k8s
from src.config.loaders import NodeConfigLoader
from src.constructs.base import BaseConstruct

# Opt-in: when this env var is set, the per-service ConfigMap is produced by the jsonnet
# `build(layout, overrides)` generator binary instead of the `apply_sequencer_overrides` placeholder
# substitution. Unset (the default) keeps the substitution path, so production is unaffected until
# the binary is provisioned in the synth pipeline.
USE_CONFIG_GENERATOR_ENV_VAR = "APOLLO_USE_CONFIG_GENERATOR"
CONFIG_GENERATOR_BINARY = "target/release/deployment_config_generator"

# The deploy's service names match the jsonnet layout's service keys, except `sierracompiler` (the
# jsonnet layout key is `sierra_compiler`). Used only for the generator's `--service` argument; the
# k8s resource name keeps the deploy name.
_GENERATOR_SERVICE_NAMES = {"sierracompiler": "sierra_compiler"}


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

        if os.environ.get(USE_CONFIG_GENERATOR_ENV_VAR):
            node_config = self._node_config_via_generator()
        else:
            node_config = self._node_config_via_substitution()

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

    def _node_config_via_substitution(self) -> dict:
        """Assemble the node config the original way: load the service's `replacer_*` files from
        `configList` and substitute the `$$$` placeholders with the merged `sequencerConfig`."""
        node_config_loader = NodeConfigLoader(
            config_list_json_path=self.service_config.config.configList,
        )
        node_config = node_config_loader.load()

        # sequencerConfig is now already merged from common into service_config.
        merged_sequencer_config = (
            self.service_config.config.sequencerConfig
            if self.service_config.config and self.service_config.config.sequencerConfig
            else {}
        )

        # Apply merged overrides (validates both unused keys and remaining placeholders).
        if merged_sequencer_config:
            return NodeConfigLoader.apply_sequencer_overrides(
                node_config,
                merged_sequencer_config,
                service_name=self.service_config.name,
                config_list_path=self.service_config.config.configList,
                layout=self.layout,
                overlays=self.overlays,
            )
        # No overrides: still validate that no placeholders remain.
        NodeConfigLoader.validate_no_remaining_placeholders(
            node_config,
            config_list_path=self.service_config.config.configList,
            layout=self.layout,
            overlays=self.overlays,
            service_name=self.service_config.name,
        )
        return node_config

    def _node_config_via_generator(self) -> dict:
        """Assemble the node config from the jsonnet `build(layout, overrides)` generator binary: the
        merged (flat dotted) `sequencerConfig` is the overrides input, and the binary prints this
        service's final node-loadable config JSON to stdout. Only the non-secret config is produced;
        secrets are mounted separately and merged by the node as a later `--config_file`."""
        merged_sequencer_config = self.service_config.config.sequencerConfig or {}
        service_name = _GENERATOR_SERVICE_NAMES.get(
            self.service_config.name, self.service_config.name
        )
        binary = os.path.join(NodeConfigLoader.ROOT_DIR, CONFIG_GENERATOR_BINARY)

        # The binary reads the overrides from a file; write the merged sequencerConfig to a temp one.
        with tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False) as overrides_file:
            json.dump(merged_sequencer_config, overrides_file)
            overrides_path = overrides_file.name
        try:
            result = subprocess.run(
                [
                    binary,
                    "--layout",
                    self.layout,
                    "--config-file",
                    overrides_path,
                    "--service",
                    service_name,
                ],
                capture_output=True,
                text=True,
            )
        finally:
            os.unlink(overrides_path)

        if result.returncode != 0:
            raise RuntimeError(
                f"config generator failed for service '{self.service_config.name}' "
                f"(layout '{self.layout}'):\n{result.stderr}"
            )
        return json.loads(result.stdout)
