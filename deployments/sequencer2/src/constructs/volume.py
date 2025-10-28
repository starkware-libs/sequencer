from constructs import Construct
from imports import k8s
from src.config import constants as const


class Volume(Construct):
    def __init__(self, scope: Construct, id: str, service_topology, labels):
        super().__init__(scope, id)

        self.service_topology = service_topology
        self.labels = labels

        self.pvc = self._get_persistent_volume_claim()

    def _get_persistent_volume_claim(self) -> k8s.KubePersistentVolumeClaim:
        return k8s.KubePersistentVolumeClaim(
            self,
            "pvc",
            metadata=k8s.ObjectMeta(name=f"{self.node.id}-data", labels=self.labels),
            spec=k8s.PersistentVolumeClaimSpec(
                storage_class_name=const.PVC_STORAGE_CLASS_NAME,
                access_modes=const.PVC_ACCESS_MODE,
                volume_mode=const.PVC_VOLUME_MODE,
                resources=k8s.ResourceRequirements(
                    requests={
                        "storage": k8s.Quantity.from_string(f"{self.service_topology.storage}Gi")
                    }
                ),
            ),
        )
