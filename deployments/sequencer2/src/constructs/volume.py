from constructs import Construct
from imports import k8s
from src.config import constants as const
from src.config.schema import ServiceConfig


class VolumeConstruct(Construct):
    def __init__(self, scope: Construct, id: str, service_config: ServiceConfig, labels):
        super().__init__(scope, id)

        self.service_config = service_config
        self.labels = labels

        self.pvc = self._get_persistent_volume_claim()

    def _get_persistent_volume_claim(self) -> k8s.KubePersistentVolumeClaim:
        return k8s.KubePersistentVolumeClaim(
            self,
            "pvc",
            metadata=k8s.ObjectMeta(name=f"sequencer-{self.service_config.name}-data", labels=self.labels),
            spec=k8s.PersistentVolumeClaimSpec(
                storage_class_name=self.service_config.persistentVolume.storageClass,
                access_modes=self.service_config.persistentVolume.accessModes,
                volume_mode=self.service_config.persistentVolume.volumeMode,
                resources=k8s.ResourceRequirements(
                    requests={
                        "storage": k8s.Quantity.from_string(f"{self.service_config.persistentVolume.size}")
                    }
                ),
            ),
        )
