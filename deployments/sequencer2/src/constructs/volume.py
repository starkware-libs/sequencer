from constructs import Construct
from imports import k8s

from src.config.schema import ServiceConfig


class VolumeConstruct(Construct):
    def __init__(self, scope: Construct, id: str, service_config: ServiceConfig, labels):
        super().__init__(scope, id)

        self.service_config = service_config
        self.labels = labels

        pv = getattr(self.service_config, "persistentVolume", None)
        if pv and getattr(pv, "enabled", False) and not getattr(pv, "existingClaim", None):
            self.pvc = self._create_persistent_volume_claim()
        else:
            self.pvc = None

    def _create_persistent_volume_claim(self) -> k8s.KubePersistentVolumeClaim:
        return k8s.KubePersistentVolumeClaim(
            self,
            "pvc",
            metadata=k8s.ObjectMeta(
                name=f"sequencer-{self.service_config.name}-pvc", labels=self.labels
            ),
            spec=k8s.PersistentVolumeClaimSpec(
                storage_class_name=self.service_config.persistentVolume.storageClass,
                access_modes=self.service_config.persistentVolume.accessModes,
                volume_mode=self.service_config.persistentVolume.volumeMode,
                resources=k8s.ResourceRequirements(
                    requests={
                        "storage": k8s.Quantity.from_string(
                            f"{self.service_config.persistentVolume.size}"
                        )
                    }
                ),
            ),
        )
