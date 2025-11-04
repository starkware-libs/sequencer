from cdk8s import ApiObjectMetadata
from imports.io.external_secrets import (
    ExternalSecretV1Beta1 as ExternalSecret,
    ExternalSecretV1Beta1Spec as ExternalSecretSpec,
    ExternalSecretV1Beta1SpecData as ExternalSecretSpecData,
    ExternalSecretV1Beta1SpecDataRemoteRef as ExternalSecretSpecDataRemoteRef,
    ExternalSecretV1Beta1SpecDataRemoteRefConversionStrategy as ExternalSecretSpecDataRemoteRefConversionStrategy,
    ExternalSecretV1Beta1SpecDataRemoteRefDecodingStrategy as ExternalSecretSpecDataRemoteRefDecodingStrategy,
    ExternalSecretV1Beta1SpecSecretStoreRef as ExternalSecretSpecSecretStoreRef,
    ExternalSecretV1Beta1SpecSecretStoreRefKind as ExternalSecretSpecSecretStoreRefKind,
    ExternalSecretV1Beta1SpecTarget as ExternalSecretSpecTarget,
)

from src.constructs.base import BaseConstruct


class ExternalSecretConstruct(BaseConstruct):
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

        self.external_secret = self._create_external_secret()

    def _create_external_secret(self) -> ExternalSecret:
        target_name = (
            self.service_config.externalSecret.targetName
            if self.service_config.externalSecret.targetName
            else f"sequencer-{self.service_config.name}-secret"
        )

        spec = ExternalSecretSpec(
            secret_store_ref=ExternalSecretSpecSecretStoreRef(
                kind=self._get_secret_store_kind(),
                name=self.service_config.externalSecret.secretStore.name,
            ),
            refresh_interval=self.service_config.externalSecret.refreshInterval,
            target=ExternalSecretSpecTarget(name=target_name),
            data=self._build_secret_data(),
        )

        # Add optional fields if configured
        if self.service_config.externalSecret.template:
            spec.template = self.service_config.externalSecret.template

        if self.service_config.externalSecret.metadata:
            spec.metadata = self.service_config.externalSecret.metadata

        if self.service_config.externalSecret.deletionPolicy != "Retain":
            spec.deletion_policy = self.service_config.externalSecret.deletionPolicy

        return ExternalSecret(
            self,
            "external-secret",
            metadata=ApiObjectMetadata(
                name=f"sequencer-{self.service_config.name}-external-secret",
                labels=self.labels,
            ),
            spec=spec,
        )

    def _get_secret_store_kind(self) -> ExternalSecretSpecSecretStoreRefKind:
        """Get the appropriate secret store kind based on configuration."""
        kind_map = {
            "ClusterSecretStore": ExternalSecretSpecSecretStoreRefKind.CLUSTER_SECRET_STORE,
            "SecretStore": ExternalSecretSpecSecretStoreRefKind.SECRET_STORE,
        }
        return kind_map.get(
            self.service_config.externalSecret.secretStore.kind,
            ExternalSecretSpecSecretStoreRefKind.CLUSTER_SECRET_STORE,
        )

    def _build_secret_data(self) -> list[ExternalSecretSpecData]:
        """Build secret data based on provider and configuration."""
        result = []
        for item in self.service_config.externalSecret.data:
            remote_ref = ExternalSecretSpecDataRemoteRef(
                key=item.remoteRef.key,
                property=item.remoteRef.property,
                conversion_strategy=ExternalSecretSpecDataRemoteRefConversionStrategy.DEFAULT,
            )

            # Add optional properties if provided
            if item.remoteRef.version:
                remote_ref.version = item.remoteRef.version

            if item.remoteRef.decoding_strategy:
                # Map string to enum
                decoding_map = {
                    "Auto": ExternalSecretSpecDataRemoteRefDecodingStrategy.AUTO,
                    "Base64": ExternalSecretSpecDataRemoteRefDecodingStrategy.BASE64,
                    "Base64Url": ExternalSecretSpecDataRemoteRefDecodingStrategy.BASE64URL,
                    "None": ExternalSecretSpecDataRemoteRefDecodingStrategy.NONE,
                }
                if item.remoteRef.decoding_strategy in decoding_map:
                    remote_ref.decoding_strategy = decoding_map[item.remoteRef.decoding_strategy]

            result.append(
                ExternalSecretSpecData(
                    secret_key=item.secretKey,
                    remote_ref=remote_ref,
                )
            )

        return result
