from pathlib import Path

import pytest
import yaml
from src.config.loaders import DeploymentConfigLoader

DEPLOYMENTS_SEQUENCER = Path(__file__).resolve().parents[1]
HYBRID_COMMON_SERVICES = DEPLOYMENTS_SEQUENCER / "configs/overlays/hybrid/common/services"
HYBRID_TESTING_NODE0_SERVICES = (
    DEPLOYMENTS_SEQUENCER / "configs/overlays/hybrid/testing/node-0/services"
)

SERVICES_DIRS = [
    pytest.param(HYBRID_COMMON_SERVICES, id="common"),
    pytest.param(HYBRID_TESTING_NODE0_SERVICES, id="testing-node-0"),
]


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


def _sequencer_config(config: dict) -> dict:
    return config.get("config", {}).get("sequencerConfig", {})


def _all_sequencer_ports(config: dict) -> list[tuple[str, int]]:
    """Return (key, port) for all sequencerConfig keys ending in '.port' with integer values."""
    result = []
    for key, value in _sequencer_config(config).items():
        if key.endswith(".port") and isinstance(value, int):
            result.append((key, value))
    return result


def _component_ports(config: dict) -> dict[str, int]:
    result = {}
    for key, value in _sequencer_config(config).items():
        parts = key.split(".")
        if (
            len(parts) == 3
            and parts[0] == "components"
            and parts[2] == "port"
            and isinstance(value, int)
        ):
            result[parts[1]] = value
    return result


def _assert_sequencer_ports_unique_within_service(service_name: str, config: dict) -> None:
    """Assert no two sequencerConfig '.port' keys in a single service share a port number."""
    seen: dict[int, str] = {}
    for key, port in _all_sequencer_ports(config):
        assert (
            port not in seen
        ), f"Service '{service_name}': port {port} assigned to both '{key}' and '{seen[port]}'"
        seen[port] = key


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
def test_sequencer_ports_unique_within_service(services_dir: Path) -> None:
    """Within each service, all sequencerConfig port values are unique.

    Catches collisions between any two port-keyed entries in the same service,
    e.g. a component port and a subsystem port accidentally sharing the same number.
    Runs on the merged config (includes expanded) so a service-local port colliding
    with a common-provided port (e.g. the monitoring port) is caught.
    """
    for service_name, config in _load_merged_service_yamls(services_dir).items():
        _assert_sequencer_ports_unique_within_service(service_name, config)


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


@pytest.mark.parametrize("services_dir", SERVICES_DIRS)
def test_component_ports_unique(services_dir: Path) -> None:
    component_to_port: dict[str, int] = {}
    port_to_component: dict[int, str] = {}

    for config in _load_service_yamls(services_dir).values():
        for component, port in _component_ports(config).items():
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


def test_within_service_collision_from_include_is_detected(tmp_path: Path) -> None:
    """The within-service check must reject a port that collides with an included common port.

    The collision is only visible once `include:` is expanded (the monitoring port lives in
    common.yaml), so this guards the merge-aware loader: raw loading would never see it. Fails
    if `_load_merged_service_yamls` ever stops expanding `include:`.
    """
    collision_port = 8082
    service_key = "components.batcher.port"
    common_key = "monitoring_endpoint_config.port"

    common = tmp_path / "common.yaml"
    common.write_text(yaml.dump({"config": {"sequencerConfig": {common_key: collision_port}}}))
    service = tmp_path / "core.yaml"
    service.write_text(
        yaml.dump(
            {
                "include": [str(common)],
                "name": "core",
                "config": {"sequencerConfig": {service_key: collision_port}},
            }
        )
    )

    # Raw loading does not expand the include, so the collision is invisible — the gap.
    raw_core = _load_service_yamls(tmp_path)["core"]
    assert _all_sequencer_ports(raw_core) == [(service_key, collision_port)]

    # Merge-aware loading pulls in the common monitoring port, exposing the collision.
    merged_core = _load_merged_service_yamls(tmp_path)["core"]
    with pytest.raises(
        AssertionError, match=f"port {collision_port} assigned to both '{service_key}'"
    ):
        _assert_sequencer_ports_unique_within_service("core", merged_core)
