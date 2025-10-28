from constructs import Construct
from imports.io.external_secrets import ExternalSecretV1Beta1 as ExternalSecret
from imports.io.external_secrets import ExternalSecretV1Beta1Spec as ExternalSecretSpec
from imports.io.external_secrets import ExternalSecretV1Beta1SpecData as ExternalSecretSpecData
from imports.io.external_secrets import (
    ExternalSecretV1Beta1SpecDataRemoteRef as ExternalSecretSpecDataRemoteRef,
)
from imports.io.external_secrets import (
    ExternalSecretV1Beta1SpecDataRemoteRefConversionStrategy as ExternalSecretSpecDataRemoteRefConversionStrategy,
)
from imports.io.external_secrets import (
    ExternalSecretV1Beta1SpecSecretStoreRef as ExternalSecretSpecSecretStoreRef,
)
from imports.io.external_secrets import (
    ExternalSecretV1Beta1SpecSecretStoreRefKind as ExternalSecretSpecSecretStoreRefKind,
)
from imports.io.external_secrets import ExternalSecretV1Beta1SpecTarget as ExternalSecretSpecTarget
from cdk8s import ApiObjectMetadata
from src.config import constants as const


class SecretConstruct(Construct):
    def __init__(self, scope: Construct, id: str, service_topology, labels):
        super().__init__(scope, id)

        self.service_topology = service_topology
        self.labels = labels

        self.external_secret = self._get_external_secret()

    def _get_external_secret(self) -> ExternalSecret:
        return ExternalSecret(
            self,
            "external-secret",
            metadata=ApiObjectMetadata(labels=self.labels),
            spec=ExternalSecretSpec(
                secret_store_ref=ExternalSecretSpecSecretStoreRef(
                    kind=ExternalSecretSpecSecretStoreRefKind.CLUSTER_SECRET_STORE,
                    name="external-secrets-project",
                ),
                refresh_interval="1m",
                target=ExternalSecretSpecTarget(
                    name=f"{self.node.id}-secret",
                ),
                data=[
                    ExternalSecretSpecData(
                        secret_key=const.SECRETS_FILE_NAME,
                        remote_ref=ExternalSecretSpecDataRemoteRef(
                            key=self.service_topology.external_secret["gcsm_key"],
                            conversion_strategy=ExternalSecretSpecDataRemoteRefConversionStrategy.DEFAULT,
                        ),
                    ),
                ],
            ),
        )
