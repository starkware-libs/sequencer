from pathlib import Path

import pytest
import yaml
from src.config.loaders import DeploymentConfigLoader
from src.config.native import build_native_config, flatten_dotted

DEPLOYMENTS_SEQUENCER = Path(__file__).resolve().parents[1]
HYBRID_COMMON_SERVICES = DEPLOYMENTS_SEQUENCER / "configs/overlays/hybrid/common/services"
HYBRID_TESTING_NODE0_SERVICES = (
    DEPLOYMENTS_SEQUENCER / "configs/overlays/hybrid/testing/node-0/services"
)

SERVICES_DIRS = [
    pytest.param(HYBRID_COMMON_SERVICES, id="common"),
    pytest.param(HYBRID_TESTING_NODE0_SERVICES, id="testing-node-0"),
]

LAYOUT = "hybrid"
# The functional `node-0` overlay is fully public (no devops checkout needed) and carries complete
# per-service port data, so it is the source for the native-config port checks.
NODE0_OVERLAYS = ["hybrid.testing.node-0"]


def _load_service_yamls(services_dir: Path) -> dict[str, dict]:
    """Raw per-service overlay YAMLs, WITHOUT expanding `include:`.

    Used for cross-service checks, which only constrain service-specific port
    allocations. Ports contributed by a shared common.yaml (e.g. the monitoring
    port) are intentionally the same on every pod, so they must not be compared
    across services.
    """
    return {
        path.stem: yaml.safe_load(path.read_text()) for path in sorted(services_dir.glob("*.yaml"))
    }


def _load_merged_service_yamls(services_dir: Path) -> dict[str, dict]:
    """Per-service YAMLs with `include:` expanded, matching the deploy-time merge.

    Mirrors `DeploymentConfigLoader._load_one_file_with_includes`, so within-service
    checks see the effective config of each pod — including ports pulled in from a
    shared common.yaml. Without this, a service-local port colliding with a
    common-provided port (e.g. the monitoring port) would slip through.
    """
    loader = DeploymentConfigLoader(
        configs_dir_path=str(services_dir), config_base_dir=str(DEPLOYMENTS_SEQUENCER)
    )
    merged_by_name: dict[str, dict] = {}
    for path in sorted(services_dir.glob("*.yaml")):
        merged = loader._load_one_file_with_includes(path, validate_final_as_service=True)
        service_name = merged.get("name") if isinstance(merged, dict) else None
        if service_name:
            merged_by_name[service_name] = merged
    return merged_by_name


def _k8s_service_ports(config: dict) -> list[int]:
    return [
        entry["port"] for entry in config.get("service", {}).get("ports", []) if "port" in entry
    ]


def _service_names(services_dir: Path) -> list[str]:
    """The `name:` field of each service overlay YAML in `services_dir`, sorted by file."""
    names = []
    for path in sorted(services_dir.glob("*.yaml")):
        document = yaml.safe_load(path.read_text()) or {}
        name = document.get("name")
        if name:
            names.append(name)
    return names


def _nonzero_port_leaves(flat: dict) -> list[tuple[str, int]]:
    """(key, port) for every '*.port' leaf with a non-zero int value.

    A `port: 0` leaf marks a component the service does not serve (disabled), so the many zeros are
    not real bindings and must not be treated as colliding.
    """
    return [
        (key, value)
        for key, value in flat.items()
        if key.endswith(".port") and isinstance(value, int) and not isinstance(value, bool) and value
    ]


def _component_ports(flat: dict) -> dict[str, int]:
    """component -> port for every non-zero `components.<c>.port` leaf in a flattened built config."""
    result = {}
    for key, value in flat.items():
        parts = key.split(".")
        if (
            len(parts) == 3
            and parts[0] == "components"
            and parts[2] == "port"
            and isinstance(value, int)
            and not isinstance(value, bool)
            and value
        ):
            result[parts[1]] = value
    return result


def _assert_ports_unique_within_service(service_name: str, flat: dict) -> None:
    """Assert no two non-zero '*.port' leaves in a single built service config share a port."""
    seen: dict[int, str] = {}
    for key, port in _nonzero_port_leaves(flat):
        assert (
            port not in seen
        ), f"Service '{service_name}': port {port} assigned to both '{key}' and '{seen[port]}'"
        seen[port] = key


@pytest.fixture(scope="module")
def node0_flat_configs() -> dict[str, dict]:
    """The built native config for each `node-0` service, flattened to dotted keys.

    This is the deployed source of truth for ports (jsonnet `build()`), replacing the former YAML
    `sequencerConfig` reads. Public overlay only — no devops checkout needed.
    """
    return {
        name: flatten_dotted(
            build_native_config(service_name=name, layout=LAYOUT, overlays=NODE0_OVERLAYS)
        )
        for name in _service_names(HYBRID_TESTING_NODE0_SERVICES)
    }


@pytest.mark.parametrize("services_dir", SERVICES_DIRS)
def test_k8s_service_ports_unique(services_dir: Path) -> None:
    seen: dict[int, str] = {}
    for service_name, config in _load_service_yamls(services_dir).items():
        for port in _k8s_service_ports(config):
            assert (
                port not in seen
            ), f"Port {port} in '{service_name}' already claimed by '{seen[port]}'"
            seen[port] = service_name


@pytest.mark.parametrize("services_dir", SERVICES_DIRS)
def test_k8s_service_ports_unique_within_service(services_dir: Path) -> None:
    """Within each service, its k8s `service.ports` are unique on the merged config.

    A single pod binds every port in its Service; merging in a common-provided port
    (e.g. the monitoring port) that already exists in the service file would collide.
    """
    for service_name, config in _load_merged_service_yamls(services_dir).items():
        seen: dict[int, str] = {}
        for port in _k8s_service_ports(config):
            assert (
                port not in seen
            ), f"Service '{service_name}': k8s port {port} listed more than once"
            seen[port] = service_name


def test_sequencer_ports_unique_within_service(node0_flat_configs: dict[str, dict]) -> None:
    """Within each built service config, all non-zero `*.port` values are unique.

    Catches collisions between any two port-keyed entries in the same service (a component port and
    a subsystem/storage-reader port accidentally sharing a number). Read from the jsonnet `build()`
    output — the deployed source of truth — so it covers both the component ports
    (`templates.componentPorts`) and the subsystem ports the override layers supply.
    """
    for service_name, flat in node0_flat_configs.items():
        _assert_ports_unique_within_service(service_name, flat)


def test_component_ports_unique(node0_flat_configs: dict[str, dict]) -> None:
    """Across services, each reactive component maps to one consistent, distinct infra port."""
    component_to_port: dict[str, int] = {}
    port_to_component: dict[int, str] = {}

    for flat in node0_flat_configs.values():
        for component, port in _component_ports(flat).items():
            if component in component_to_port:
                assert component_to_port[component] == port, (
                    f"Component '{component}' has inconsistent ports: "
                    f"{component_to_port[component]} vs {port}"
                )
            else:
                assert (
                    port not in port_to_component
                ), f"Port {port} used by both '{component}' and '{port_to_component[port]}'"
                component_to_port[component] = port
                port_to_component[port] = component


def test_within_service_port_collision_is_detected() -> None:
    """The within-service check ignores disabled components (port 0) but rejects a real collision.

    Guards the zero-filter (the load-bearing new behavior): a built config emits every component the
    service does not serve as `port: 0`, so those must not be flagged; two non-zero ports sharing a
    value must.
    """
    # Disabled components (port 0) must not be treated as colliding.
    no_collision = {
        "components": {"a": {"port": 0}, "b": {"port": 0}},
        "http_server_config": {"static_config": {"port": 8080}},
    }
    _assert_ports_unique_within_service("ok", flatten_dotted(no_collision))

    # Two non-zero ports sharing a value must raise.
    collision = {
        "components": {"batcher": {"port": 55000}},
        "monitoring_endpoint_config": {"port": 55000},
    }
    with pytest.raises(AssertionError, match="port 55000 assigned to both"):
        _assert_ports_unique_within_service("bad", flatten_dotted(collision))
