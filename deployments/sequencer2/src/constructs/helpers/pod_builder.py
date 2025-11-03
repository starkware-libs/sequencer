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
            priority_class_name=self._get_priority_class_name(),
            security_context=self._build_security_context(),
            image_pull_secrets=self._build_image_pull_secrets(),
            volumes=self._build_volumes(),
            tolerations=self._build_tolerations(),
            node_selector=self.service_config.nodeSelector,
            affinity=self._build_affinity(),
            containers=[self._build_container()],
        )

    def _get_priority_class_name(self) -> str | None:
        """Get the priority class name to use in pod spec."""
        if not self.service_config.priorityClass or not self.service_config.priorityClass.enabled:
            return None

        # If existingPriorityClass is set, use it
        if self.service_config.priorityClass.existingPriorityClass:
            return self.service_config.priorityClass.existingPriorityClass

        # Otherwise, use the name of the PriorityClass we created (or default name)
        if self.service_config.priorityClass.name:
            return self.service_config.priorityClass.name

        # Default name follows the pattern
        return f"sequencer-{self.service_config.name}-priorityclass"

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
            args=self._build_container_args(),
            env=self._build_container_env(),
            ports=self._build_container_ports(),
            startup_probe=self._build_http_probe(self.service_config.startupProbe),
            readiness_probe=self._build_http_probe(self.service_config.readinessProbe),
            liveness_probe=self._build_http_probe(self.service_config.livenessProbe),
            volume_mounts=self._build_volume_mounts(),
            resources=self._build_container_resources(),
        )

    def _build_container_args(self) -> list[str]:
        """Build container arguments, always including --config_file with fixed file paths."""
        args = []

        # Add --config_file /config/sequencer/presets/config.json (ConfigMap)
        if self.service_config.config and self.service_config.config.configPaths:
            mount_path = (
                self.service_config.config.mountPath
                if self.service_config.config.mountPath
                else "/config/sequencer/presets"
            )
            args.append("--config_file")
            args.append(f"{mount_path}/config.json")

        # Add --config_file /etc/secrets/secret.json (Secret)
        if (
            self.service_config.secret
            and self.service_config.secret.enabled
            and (self.service_config.secret.data or self.service_config.secret.stringData)
        ):
            mount_path = (
                self.service_config.secret.mountPath
                if self.service_config.secret.mountPath
                else "/etc/secrets"
            )
            args.append("--config_file")
            args.append(f"{mount_path}/secret.json")

        # Add --config_file /etc/secrets/external-secret.json (ExternalSecret)
        if (
            self.service_config.externalSecret
            and self.service_config.externalSecret.enabled
            and self.service_config.externalSecret.data
        ):
            mount_path = (
                self.service_config.externalSecret.mountPath
                if self.service_config.externalSecret.mountPath
                else "/etc/secrets"
            )
            args.append("--config_file")
            args.append(f"{mount_path}/external-secret.json")

        # Append any additional args from node.yaml
        if self.service_config.args:
            args.extend(self.service_config.args)

        return args

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

        # Auto-mount ConfigMap if config exists (as config.json file)
        if self.service_config.config and self.service_config.config.configPaths:
            # Default mountPath is "/config/sequencer/presets"
            mount_path = "/config/sequencer/presets"
            if self.service_config.config.mountPath:
                mount_path = self.service_config.config.mountPath

            volume_mounts.append(
                k8s.VolumeMount(
                    name=f"sequencer-{self.service_config.name}-config",
                    mount_path=f"{mount_path}/config.json",
                    sub_path="config_json",  # Matches the ConfigMap key (dots not allowed in K8s keys)
                    read_only=True,
                )
            )

        # Mount Secret if enabled (always mounted as secret.json regardless of key name)
        if (
            self.service_config.secret
            and self.service_config.secret.enabled
            and (self.service_config.secret.data or self.service_config.secret.stringData)
        ):
            secret_name = (
                self.service_config.secret.name or f"sequencer-{self.service_config.name}-secret"
            )
            secret_volume_name = f"{secret_name}-volume"
            # Default mountPath is "/etc/secrets"
            mount_path = (
                self.service_config.secret.mountPath
                if self.service_config.secret.mountPath
                else "/etc/secrets"
            )

            # Get the first available key (any key name is fine)
            secret_key = None
            if self.service_config.secret.stringData:
                secret_key = next(iter(self.service_config.secret.stringData.keys()))
            elif self.service_config.secret.data:
                secret_key = next(iter(self.service_config.secret.data.keys()))

            if secret_key:
                # Mount whatever key they provided as secret.json using subPath
                volume_mounts.append(
                    k8s.VolumeMount(
                        name=secret_volume_name,
                        mount_path=f"{mount_path}/secret.json",
                        sub_path=secret_key,
                        read_only=True,
                    )
                )

        # Mount ExternalSecret if enabled (always mounted as external-secret.json regardless of key name)
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
            # Default mountPath is "/etc/secrets"
            mount_path = (
                self.service_config.externalSecret.mountPath
                if self.service_config.externalSecret.mountPath
                else "/etc/secrets"
            )

            # Get the first available key (any key name is fine)
            external_secret_key = (
                self.service_config.externalSecret.data[0].secretKey
                if self.service_config.externalSecret.data
                else None
            )

            if external_secret_key:
                # Mount whatever key they provided as external-secret.json using subPath
                volume_mounts.append(
                    k8s.VolumeMount(
                        name=external_secret_volume_name,
                        mount_path=f"{mount_path}/external-secret.json",
                        sub_path=external_secret_key,
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

        # Always create ConfigMap volume if config exists
        if self.service_config.config and self.service_config.config.configPaths:
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
