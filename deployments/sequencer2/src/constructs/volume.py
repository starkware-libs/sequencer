from imports import k8s

from src.constructs.base import BaseConstruct


class VolumeConstruct(BaseConstruct):
    def __init__(
        self,
        scope,
        id: str,
        common_config,
        service_config,
        labels,
        monitoring_endpoint_port,
    ):
        super().__init__(
            scope,
            id,
            common_config,
            service_config,
            labels,
            monitoring_endpoint_port,
        )

        pv = self.service_config.persistentVolume
        if pv and pv.enabled and not pv.existingClaim:
            self.pvc = self._create_persistent_volume_claim()
        else:
            self.pvc = None

    def _create_persistent_volume_claim(self) -> k8s.KubePersistentVolumeClaim:
        return k8s.KubePersistentVolumeClaim(
            self,
            "pvc",
            metadata=k8s.ObjectMeta(
                name=f"sequencer-{self.service_config.name}-data", labels=self.labels
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
