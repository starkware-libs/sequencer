import dataclasses
from typing import Optional, Dict, Any

from services.config import SequencerConfig
from services import const


@dataclasses.dataclass
class ServiceTopology:
    config: SequencerConfig
    image: str
    controller: str
    replicas: int
    autoscale: bool
    ports: Optional[Dict[str, int]]
    ingress: Optional[Dict[Any, Any]]
    toleration: Optional[str]
    storage: Optional[int]
    resources: Optional[Dict[str, Dict[str, int]]]
    external_secret: Optional[Dict[str, str]]

    def __post_init__(self):
        if self.ports:
            self.seen_ports = set()
            for port_name, port_number in self.ports.items():
                self._validate_port(port_name, port_number)

    def _validate_port(self, port_name: str, port_number: int) -> bool:
        assert (
            port_number not in self.seen_ports
        ), f"Duplicate port {port_number} found in service ports. Please ensure all ports are unique."
        assert (
            port_name != const.MONITORING_ENDPOINT_PORT_NAME
        ), f"{const.MONITORING_ENDPOINT_PORT_NAME} is reserved port name. Remove it from deployment_config ports section."
        assert (
            port_number != const.MONITORING_ENDPOINT_PORT_NUMBER
        ), f"port {port_number} is reserved port number. Remove it from deployment_config ports section."
        assert (
            1 <= port_number <= 65535
        ), f"Port {port_number} is out of valid range. Ports must be between 1 and 65535."

        self.seen_ports.add(port_number)
