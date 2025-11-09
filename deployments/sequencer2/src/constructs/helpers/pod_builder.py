from imports import k8s
from src.config.schema import Probe as ProbeConfig
from src.config.schema import ServiceConfig


class PodBuilder:
    """Helper class for building Kubernetes Pod specifications."""

    def __init__(
        self,
        service_config: ServiceConfig,
        labels: dict[str, str],
        monitoring_endpoint_port: int,
    ):
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
        if not self.service_config.image:
            raise ValueError(f"Image configuration is required for service {self.service_config.name}")
        image = f"{self.service_config.image.repository}:{self.service_config.image.tag}"
        # Use None for command if empty list (allows image default)
        command = self.service_config.command if self.service_config.command else None
        return k8s.Container(
            name=f"sequencer-{self.service_config.name}",
            image=image,
            image_pull_policy=self.service_config.image.imagePullPolicy,
            command=command,
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

        # Add --config_file /config/sequencer/presets/config (ConfigMap)
        # Note: node version uses directory mount, so path is just the directory + "config"
        if self.service_config.config and self.service_config.config.configList:
            mount_path = (
                self.service_config.config.mountPath
                if self.service_config.config.mountPath
                else "/config/sequencer/presets/"
            )
            # Ensure mount_path ends with / for proper path joining
            if not mount_path.endswith("/"):
                mount_path = mount_path + "/"
            args.append("--config_file")
            args.append(f"{mount_path}config")

        # Add --config_file /etc/secrets/secrets.json (ExternalSecret)
        # Note: node version uses directory mount, so path is /etc/secrets/secrets.json
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
            args.append(f"{mount_path}/secrets.json")

        # Add --config_file /etc/secrets/secret.json (Secret) - if using regular Secret
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
        return [{"name": secret} for secret in self.service_config.imagePullSecrets]

    def _build_container_ports(self) -> list[k8s.ContainerPort]:
        """Build container ports from service configuration."""
        ports = []
        if self.service_config.service and self.service_config.service.ports:
            for port_config in self.service_config.service.ports:
                # Node version doesn't use port names or protocol
                ports.append(
                    k8s.ContainerPort(
                        container_port=port_config.port,
                    )
                )
        return ports

    def _build_http_probe(self, probe_config: ProbeConfig) -> k8s.Probe | None:
        """Build HTTP probe if enabled."""
        if not probe_config or not probe_config.enabled:
            return None

        # Always use monitoring_endpoint_port for probes
        port_number = self.monitoring_endpoint_port

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

        # Auto-mount ConfigMap if config exists (as directory, not file)
        if self.service_config.config and self.service_config.config.configList:
            # Default mountPath is "/config/sequencer/presets/" (with trailing slash to match node/)
            mount_path = (
                self.service_config.config.mountPath
                if self.service_config.config.mountPath
                else "/config/sequencer/presets/"
            )

            # Mount as directory (node version uses directory mount)
            volume_mounts.append(
                k8s.VolumeMount(
                    name=f"sequencer-{self.service_config.name}-config",
                    mount_path=mount_path,
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

        # Mount ExternalSecret if enabled (mount as directory, not file)
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
            external_secret_volume_name = external_secret_target_name
            # Default mountPath is "/etc/secrets"
            mount_path = (
                self.service_config.externalSecret.mountPath
                if self.service_config.externalSecret.mountPath
                else "/etc/secrets"
            )

            # Mount as directory (node version uses directory mount)
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
                    name=f"sequencer-{self.service_config.name}-data",
                    mount_path=self.service_config.persistentVolume.mountPath,
                    read_only=False,
                )
            )

        return volume_mounts

    def _build_volumes(self) -> list[k8s.Volume]:
        """Build volumes for the pod."""
        volumes: list[k8s.Volume] = []

        # Always create ConfigMap volume if config exists
        if self.service_config.config and self.service_config.config.configList:
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
        # Node version uses items with key/path and defaultMode: 400
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
            external_secret_volume_name = external_secret_target_name

            # Get the first secret key
            secret_key = (
                self.service_config.externalSecret.data[0].secretKey
                if self.service_config.externalSecret.data
                else "secrets.json"
            )

            volumes.append(
                k8s.Volume(
                    name=external_secret_volume_name,
                    secret=k8s.SecretVolumeSource(
                        secret_name=external_secret_target_name,
                        default_mode=400,  # Match node/ format (Kubernetes interprets as octal)
                        items=[
                            k8s.KeyToPath(
                                key=secret_key,
                                path="secrets.json",
                            )
                        ],
                    ),
                )
            )

        # Create persistentVolume volume if enabled
        if self.service_config.persistentVolume and self.service_config.persistentVolume.enabled:
            pvc_name = (
                self.service_config.persistentVolume.existingClaim
                if self.service_config.persistentVolume.existingClaim
                else f"sequencer-{self.service_config.name}-data"
            )
            volumes.append(
                k8s.Volume(
                    name=f"sequencer-{self.service_config.name}-data",
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
        """Build pod affinity configuration, merging affinity and podAntiAffinity."""
        # Start with existing affinity if present
        base_affinity = (
            k8s.Affinity.from_json(self.service_config.affinity)
            if self.service_config.affinity
            else k8s.Affinity()
        )

        # Build pod anti-affinity from structured config if enabled
        pod_anti_affinity = None
        if self.service_config.podAntiAffinity and self.service_config.podAntiAffinity.enabled:
            pod_anti_affinity = self._build_pod_anti_affinity(self.service_config.podAntiAffinity)

            # Merge with existing pod anti-affinity if present
            if base_affinity.pod_anti_affinity:
                # Merge preferred rules
                existing_preferred = (
                    base_affinity.pod_anti_affinity.preferred_during_scheduling_ignored_during_execution
                    or []
                )
                new_preferred = (
                    pod_anti_affinity.preferred_during_scheduling_ignored_during_execution or []
                )
                merged_preferred = existing_preferred + new_preferred

                # Merge required rules
                existing_required = (
                    base_affinity.pod_anti_affinity.required_during_scheduling_ignored_during_execution
                    or []
                )
                new_required = (
                    pod_anti_affinity.required_during_scheduling_ignored_during_execution or []
                )
                merged_required = existing_required + new_required

                # Create merged pod anti-affinity
                pod_anti_affinity = k8s.PodAntiAffinity(
                    preferred_during_scheduling_ignored_during_execution=(
                        merged_preferred if merged_preferred else None
                    ),
                    required_during_scheduling_ignored_during_execution=(
                        merged_required if merged_required else None
                    ),
                )

            # Create new Affinity object with merged pod anti-affinity
            return k8s.Affinity(
                node_affinity=base_affinity.node_affinity,
                pod_affinity=base_affinity.pod_affinity,
                pod_anti_affinity=pod_anti_affinity,
            )
        elif pod_anti_affinity:
            # Only pod anti-affinity, no existing affinity
            return k8s.Affinity(
                node_affinity=None,
                pod_affinity=None,
                pod_anti_affinity=pod_anti_affinity,
            )

        # Return None if affinity is empty, otherwise return the configured affinity
        if (
            not base_affinity.node_affinity
            and not base_affinity.pod_affinity
            and not base_affinity.pod_anti_affinity
        ):
            return None

        return base_affinity

    def _build_pod_anti_affinity(self, pod_anti_affinity_config) -> k8s.PodAntiAffinity:
        """Build Kubernetes PodAntiAffinity from structured configuration."""
        preferred = []
        required = []

        # Build preferred rules
        for rule in pod_anti_affinity_config.preferred:
            if rule.weight is None:
                rule.weight = 100  # Default weight if not specified

            # Build label selector from dict, defaulting to pod labels if empty
            label_selector = self._build_label_selector(
                rule.labelSelector, default_match_labels=self.labels
            )

            preferred.append(
                k8s.WeightedPodAffinityTerm(
                    weight=rule.weight,
                    pod_affinity_term=k8s.PodAffinityTerm(
                        label_selector=label_selector,
                        topology_key=rule.topologyKey,
                    ),
                )
            )

        # Build required rules
        for rule in pod_anti_affinity_config.required:
            # Build label selector from dict, defaulting to pod labels if empty
            label_selector = self._build_label_selector(
                rule.labelSelector, default_match_labels=self.labels
            )

            required.append(
                k8s.PodAffinityTerm(
                    label_selector=label_selector,
                    topology_key=rule.topologyKey,
                )
            )

        return k8s.PodAntiAffinity(
            preferred_during_scheduling_ignored_during_execution=preferred if preferred else None,
            required_during_scheduling_ignored_during_execution=required if required else None,
        )

    def _build_label_selector(
        self, label_selector_dict: dict, default_match_labels: dict | None = None
    ) -> k8s.LabelSelector:
        """Build Kubernetes LabelSelector from dictionary.

        Args:
            label_selector_dict: Dictionary with matchLabels and/or matchExpressions
            default_match_labels: Default matchLabels to use if label_selector_dict is empty.
                                  This ensures labelSelector stays in sync with pod labels.
        """
        match_labels = label_selector_dict.get("matchLabels", {})
        match_expressions = label_selector_dict.get("matchExpressions", [])

        # If no matchLabels specified and no matchExpressions, use default (pod labels)
        # This ensures labelSelector automatically matches pod labels, preventing sync issues
        if not match_labels and not match_expressions and default_match_labels:
            match_labels = default_match_labels

        # Convert matchExpressions to LabelSelectorRequirement if provided
        label_selector_requirements = None
        if match_expressions:
            label_selector_requirements = [
                k8s.LabelSelectorRequirement(
                    key=expr.get("key"),
                    operator=expr.get("operator"),
                    values=expr.get("values", []),
                )
                for expr in match_expressions
            ]

        return k8s.LabelSelector(
            match_labels=match_labels,
            match_expressions=label_selector_requirements,
        )
