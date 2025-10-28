
import json

from constructs import Construct
from imports import k8s


class ConfigMapConstruct(Construct):
    def __init__(self, scope: Construct, id: str, node_config):
        super().__init__(scope, id)

        self.config_map = k8s.KubeConfigMap(
            self,
            "configmap",
            metadata=k8s.ObjectMeta(name=f"{self.node.id}-config"),
            data=dict(config=json.dumps(node_config, indent=2)),
        )
