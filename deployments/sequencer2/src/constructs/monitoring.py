
from cdk8s import ApiObjectMetadata
from constructs import Construct
from imports.com.googleapis.monitoring import (
    PodMonitoring,
    PodMonitoringSpec,
    PodMonitoringSpecEndpoints,
    PodMonitoringSpecEndpointsPort,
    PodMonitoringSpecSelector,
)
from src.config import constants as const


class PodMonitoringConstruct(Construct):
    def __init__(self, scope: Construct, id: str, labels, monitoring_endpoint_port):
        super().__init__(scope, id)

        self.podmonitoring = PodMonitoring(
            self,
            "pod-monitoring",
            metadata=ApiObjectMetadata(
                labels=labels,
            ),
            spec=PodMonitoringSpec(
                selector=PodMonitoringSpecSelector(match_labels=labels),
                endpoints=[
                    PodMonitoringSpecEndpoints(
                        port=PodMonitoringSpecEndpointsPort.from_number(
                            monitoring_endpoint_port
                        ),
                        interval="10s",
                        path=const.MONITORING_METRICS_ENDPOINT,
                    )
                ],
            ),
        )
