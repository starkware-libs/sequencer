from imports import k8s

from src.constructs.base import BaseConstruct


class ServiceAccountConstruct(BaseConstruct):
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

        self.service_account = self._create_service_account()

    def _create_service_account(self) -> k8s.KubeServiceAccount:
        # Merge service account labels with common labels
        sa_labels = {**self.labels, **self.service_config.serviceAccount.labels}

        spec = {}

        # Add automount service account token if specified
        if self.service_config.serviceAccount.automountServiceAccountToken is not None:
            spec["automount_service_account_token"] = (
                self.service_config.serviceAccount.automountServiceAccountToken
            )

        # Add image pull secrets if specified
        if self.service_config.serviceAccount.imagePullSecrets:
            spec["image_pull_secrets"] = [
                k8s.LocalObjectReference(name=secret_name)
                for secret_name in self.service_config.serviceAccount.imagePullSecrets
            ]

        # Add secrets if specified
        if self.service_config.serviceAccount.secrets:
            spec["secrets"] = [
                k8s.ObjectReference.from_json(secret_config)
                for secret_config in self.service_config.serviceAccount.secrets
            ]

        return k8s.KubeServiceAccount(
            self,
            "service-account",
            metadata=k8s.ObjectMeta(
                name=self.service_config.serviceAccount.name
                or f"sequencer-{self.service_config.name}-sa",
                labels=sa_labels,
                annotations=self.service_config.serviceAccount.annotations,
            ),
            **spec,  # Pass spec fields directly to KubeServiceAccount
        )
