import base64
from imports import k8s

from src.constructs.base import BaseConstruct


class SecretConstruct(BaseConstruct):
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

        self.secret = self._create_secret()

    def _create_secret(self) -> k8s.KubeSecret:
        # Merge secret labels with common labels
        secret_labels = {**self.labels, **self.service_config.secret.labels}

        # Prepare data - encode stringData to base64 for data field
        data = {}
        if self.service_config.secret.stringData:
            for key, value in self.service_config.secret.stringData.items():
                data[key] = base64.b64encode(value.encode("utf-8")).decode("utf-8")

        # Add any existing data (already base64 encoded)
        data.update(self.service_config.secret.data)

        # Ensure secret.json key exists (validation is done in schema, but double-check)
        if not data and not self.service_config.secret.stringData:
            raise ValueError("Secret must have data or stringData with at least secret.json key")

        return k8s.KubeSecret(
            self,
            "secret",
            metadata=k8s.ObjectMeta(
                name=self.service_config.secret.name
                or f"sequencer-{self.service_config.name}-secret",
                labels=secret_labels,
                annotations=self.service_config.secret.annotations,
            ),
            type=self.service_config.secret.type,
            data=data if data else None,
            string_data=(
                self.service_config.secret.stringData
                if self.service_config.secret.stringData
                else None
            ),
            immutable=self.service_config.secret.immutable,
        )
