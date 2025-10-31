from imports import k8s

from src.config.schema import CommonConfig, Probe as ProbeConfig, ServiceConfig


class PodBuilder:
    """Helper class for building Kubernetes Pod specifications."""

    def __init__(
        self,
        common_config: CommonConfig,
        service_config: ServiceConfig,
        labels: dict[str, str],
        monitoring_endpoint_port: int,
    ):
        self.common_config = common_config
        self.service_config = service_config
        self.labels = labels
        self.monitoring_endpoint_port = monitoring_endpoint_port

    def build_pod_spec(self) -> k8s.PodSpec:
        """Build a complete PodSpec with all necessary configurations."""
        return k8s.PodSpec(
            service_account_name=self._get_service_account_name(),
            termination_grace_period_seconds=self.service_config.terminationGracePeriodSeconds,
            priority_class_name=self.service_config.priorityClassName,
            security_context=self._build_security_context(),
            image_pull_secrets=self._build_image_pull_secrets(),
            volumes=self._build_volumes(),
            tolerations=self._build_tolerations(),
            node_selector=self.service_config.nodeSelector,
            affinity=self._build_affinity(),
            containers=[self._build_container()],
        )

    def build_container(self) -> k8s.Container:
        """Build a single container specification."""
        return self._build_container()

    def _build_container(self) -> k8s.Container:
        """Build the main application container."""
        image = f"{self.common_config.image.repository}:{self.common_config.image.tag}"
        return k8s.Container(
            name=self.service_config.name,
            image=image,
            image_pull_policy=self.common_config.image.imagePullPolicy,
            command=self.service_config.command,
            env=self._build_container_env(),
            ports=self._build_container_ports(),
            startup_probe=self._build_http_probe(self.service_config.startupProbe),
            readiness_probe=self._build_http_probe(self.service_config.readinessProbe),
            liveness_probe=self._build_http_probe(self.service_config.livenessProbe),
            volume_mounts=self._build_volume_mounts(),
            resources=self._build_container_resources(),
        )

    def _get_service_account_name(self) -> str | None:
        """Get the service account name if configured."""
        return (
            self.service_config.serviceAccount.name if self.service_config.serviceAccount else None
        )

    def _build_security_context(self) -> k8s.PodSecurityContext | None:
        """Build the pod security context if configured."""
        if not self.service_config.securityContext:
            return None

        return k8s.PodSecurityContext(
            fs_group=self.service_config.securityContext.fsGroup,
            run_as_group=self.service_config.securityContext.runAsGroup,
            run_as_user=self.service_config.securityContext.runAsUser,
            run_as_non_root=self.service_config.securityContext.runAsNonRoot,
        )

    def _build_image_pull_secrets(self) -> list[dict[str, str]]:
        """Build image pull secrets list."""
        return [{"name": secret} for secret in self.common_config.imagePullSecrets]

    def _build_container_ports(self) -> list[k8s.ContainerPort]:
        """Build container ports from service configuration."""
        ports = []
        if self.service_config.service and self.service_config.service.ports:
            for port_config in self.service_config.service.ports:
                ports.append(
                    k8s.ContainerPort(
                        container_port=port_config.port,
                        name=port_config.name,
                        protocol=port_config.protocol,
                    )
                )
        return ports

    def _build_http_probe(self, probe_config: ProbeConfig) -> k8s.Probe | None:
        """Build HTTP probe if enabled."""
        if not probe_config or not probe_config.enabled:
            return None

        # Find the port number from the service definition
        port_number = self.monitoring_endpoint_port
        if self.service_config.service and self.service_config.service.ports:
            # Assuming the first port is the target for the probe if not specified otherwise
            port_number = self.service_config.service.ports[0].port

        return k8s.Probe(
            http_get=k8s.HttpGetAction(
                path=probe_config.path,
                port=k8s.IntOrString.from_number(port_number),
                scheme=probe_config.probeScheme,
            ),
            period_seconds=probe_config.periodSeconds,
            failure_threshold=probe_config.failureThreshold,
            success_threshold=probe_config.successThreshold,
            timeout_seconds=probe_config.timeoutSeconds,
        )

    def _build_volume_mounts(self) -> list[k8s.VolumeMount]:
        """Build volume mounts for the container."""
        volume_mounts: list[k8s.VolumeMount] = []

        # Always mount ConfigMap if configPaths exist
        if self.service_config.configPaths:
            volume_mounts.append(
                k8s.VolumeMount(
                    name=f"sequencer-{self.service_config.name}-config",
                    mount_path="/app/config",
                    read_only=True,
                )
            )

        # Mount Secret if enabled
        if (
            self.service_config.secret
            and self.service_config.secret.enabled
            and (self.service_config.secret.data or self.service_config.secret.stringData)
        ):
            secret_name = (
                self.service_config.secret.name or f"sequencer-{self.service_config.name}-secret"
            )
            secret_volume_name = f"{secret_name}-volume"
            mount_path = (
                self.service_config.secret.mountPath
                if self.service_config.secret.mountPath
                else "/app/secrets"
            )
            volume_mounts.append(
                k8s.VolumeMount(
                    name=secret_volume_name,
                    mount_path=mount_path,
                    read_only=True,
                )
            )

        # Mount ExternalSecret if enabled
        if (
            self.service_config.externalSecret
            and self.service_config.externalSecret.enabled
            and self.service_config.externalSecret.data
        ):
            external_secret_target_name = (
                self.service_config.externalSecret.targetName
                if self.service_config.externalSecret.targetName
                else f"sequencer-{self.service_config.name}-secret"
            )
            external_secret_volume_name = f"{external_secret_target_name}-secrets-volume"
            mount_path = (
                self.service_config.externalSecret.mountPath
                if self.service_config.externalSecret.mountPath
                else "/app/secrets"
            )
            volume_mounts.append(
                k8s.VolumeMount(
                    name=external_secret_volume_name,
                    mount_path=mount_path,
                    read_only=True,
                )
            )

        # Mount persistentVolume if enabled
        if (
            self.service_config.persistentVolume
            and self.service_config.persistentVolume.enabled
            and self.service_config.persistentVolume.mountPath
        ):
            volume_mounts.append(
                k8s.VolumeMount(
                    name=f"sequencer-{self.service_config.name}-pvc",
                    mount_path=self.service_config.persistentVolume.mountPath,
                )
            )

        return volume_mounts

    def _build_volumes(self) -> list[k8s.Volume]:
        """Build volumes for the pod."""
        volumes: list[k8s.Volume] = []

        # Always create ConfigMap volume if configPaths exist
        if self.service_config.configPaths:
            volumes.append(
                k8s.Volume(
                    name=f"sequencer-{self.service_config.name}-config",
                    config_map=k8s.ConfigMapVolumeSource(
                        name=f"sequencer-{self.service_config.name}-config"
                    ),
                )
            )

        # Create Secret volume if enabled
        if (
            self.service_config.secret
            and self.service_config.secret.enabled
            and (self.service_config.secret.data or self.service_config.secret.stringData)
        ):
            secret_name = (
                self.service_config.secret.name or f"sequencer-{self.service_config.name}-secret"
            )
            secret_volume_name = f"{secret_name}-volume"
            volumes.append(
                k8s.Volume(
                    name=secret_volume_name,
                    secret=k8s.SecretVolumeSource(secret_name=secret_name),
                )
            )

        # Create ExternalSecret volume if enabled
        # ExternalSecret creates a target Secret that we mount
        if (
            self.service_config.externalSecret
            and self.service_config.externalSecret.enabled
            and self.service_config.externalSecret.data
        ):
            external_secret_target_name = (
                self.service_config.externalSecret.targetName
                if self.service_config.externalSecret.targetName
                else f"sequencer-{self.service_config.name}-secret"
            )
            external_secret_volume_name = f"{external_secret_target_name}-secrets-volume"
            volumes.append(
                k8s.Volume(
                    name=external_secret_volume_name,
                    secret=k8s.SecretVolumeSource(secret_name=external_secret_target_name),
                )
            )

        # Create persistentVolume volume if enabled
        if self.service_config.persistentVolume and self.service_config.persistentVolume.enabled:
            pvc_name = (
                self.service_config.persistentVolume.existingClaim
                if self.service_config.persistentVolume.existingClaim
                else f"sequencer-{self.service_config.name}-pvc"
            )
            volumes.append(
                k8s.Volume(
                    name=f"sequencer-{self.service_config.name}-pvc",
                    persistent_volume_claim=k8s.PersistentVolumeClaimVolumeSource(
                        claim_name=pvc_name
                    ),
                )
            )

        return volumes

    def _build_container_resources(self) -> k8s.ResourceRequirements | None:
        """Build container resource requirements."""
        if not self.service_config.resources:
            return None

        requests = self.service_config.resources.requests
        limits = self.service_config.resources.limits

        return k8s.ResourceRequirements(
            requests=(
                {
                    "cpu": k8s.Quantity.from_string(str(requests.cpu)),
                    "memory": k8s.Quantity.from_string(requests.memory),
                }
                if requests
                else None
            ),
            limits=(
                {
                    "cpu": k8s.Quantity.from_string(str(limits.cpu)),
                    "memory": k8s.Quantity.from_string(limits.memory),
                }
                if limits
                else None
            ),
        )

    def _build_container_env(self) -> list[k8s.EnvVar]:
        """Build environment variables for the container."""
        env = []
        for env_var in self.service_config.env:
            # The pydantic model can handle different structures, this is a simple example
            if "name" in env_var and "value" in env_var:
                env.append(k8s.EnvVar(name=env_var["name"], value=str(env_var["value"])))
        return env

    def _build_tolerations(self) -> list[k8s.Toleration]:
        """Build pod tolerations."""
        tolerations = []
        for toleration_config in self.service_config.tolerations:
            tolerations.append(
                k8s.Toleration(
                    key=toleration_config.get("key"),
                    operator=toleration_config.get("operator"),
                    value=toleration_config.get("value"),
                    effect=toleration_config.get("effect"),
                )
            )
        return tolerations

    def _build_affinity(self) -> k8s.Affinity | None:
        """Build pod affinity configuration."""
        return (
            k8s.Affinity.from_json(self.service_config.affinity)
            if self.service_config.affinity
            else None
        )
