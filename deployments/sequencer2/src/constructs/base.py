from constructs import Construct
from imports import k8s

from src.config.schema import CommonConfig, ServiceConfig


class BaseConstruct(Construct):
    def __init__(
        self,
        scope: Construct,
        id: str,
        common_config: CommonConfig,
        service_config: ServiceConfig,
        labels,
        monitoring_endpoint_port,
    ):
        super().__init__(scope, id)
        self.common_config = common_config
        self.service_config = service_config
        self.labels = labels
        self.monitoring_endpoint_port = monitoring_endpoint_port
