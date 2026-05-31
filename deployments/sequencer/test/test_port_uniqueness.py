from pathlib import Path

import pytest
import yaml

DEPLOYMENTS_SEQUENCER = Path(__file__).parents[1]
HYBRID_COMMON_SERVICES = DEPLOYMENTS_SEQUENCER / "configs/overlays/hybrid/common/services"
HYBRID_TESTING_NODE0_SERVICES = (
    DEPLOYMENTS_SEQUENCER / "configs/overlays/hybrid/testing/node-0/services"
)

SERVICES_DIRS = [
    pytest.param(HYBRID_COMMON_SERVICES, id="common"),
    pytest.param(HYBRID_TESTING_NODE0_SERVICES, id="testing-node-0"),
]


def _load_service_yamls(services_dir: Path) -> dict[str, dict]:
    return {
        path.stem: yaml.safe_load(path.read_text())
        for path in sorted(services_dir.glob("*.yaml"))
    }


def _k8s_service_ports(config: dict) -> list[int]:
    return [
        entry["port"]
        for entry in config.get("service", {}).get("ports", [])
        if "port" in entry
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


@pytest.mark.parametrize("services_dir", SERVICES_DIRS)
def test_k8s_service_ports_unique(services_dir: Path) -> None:
    seen: dict[int, str] = {}
    for service_name, config in _load_service_yamls(services_dir).items():
        for port in _k8s_service_ports(config):
            assert port not in seen, (
                f"Port {port} in '{service_name}' already claimed by '{seen[port]}'"
            )
            seen[port] = service_name


@pytest.mark.parametrize("services_dir", SERVICES_DIRS)
def test_sequencer_ports_unique_within_service(services_dir: Path) -> None:
    """Within each service, all sequencerConfig port values are unique.

    Catches collisions between any two port-keyed entries in the same service,
    e.g. a component port and a subsystem port accidentally sharing the same number.
    """
    for service_name, config in _load_service_yamls(services_dir).items():
        seen: dict[int, str] = {}
        for key, port in _all_sequencer_ports(config):
            assert port not in seen, (
                f"Service '{service_name}': port {port} assigned to both "
                f"'{key}' and '{seen[port]}'"
            )
            seen[port] = key


@pytest.mark.parametrize("services_dir", SERVICES_DIRS)
def test_component_ports_unique(services_dir: Path) -> None:
    component_to_port: dict[str, int] = {}
    port_to_component: dict[int, str] = {}

    for _, config in _load_service_yamls(services_dir).items():
        for component, port in _component_ports(config).items():
            if component in component_to_port:
                assert component_to_port[component] == port, (
                    f"Component '{component}' has inconsistent ports: "
                    f"{component_to_port[component]} vs {port}"
                )
            else:
                assert port not in port_to_component, (
                    f"Port {port} used by both '{component}' and '{port_to_component[port]}'"
                )
                component_to_port[component] = port
                port_to_component[port] = component
