from imports import k8s
from src.constructs.base import BaseConstruct


class PodDisruptionBudgetConstruct(BaseConstruct):
    def __init__(
        self,
        scope,
        id: str,service_config,
        labels,
        monitoring_endpoint_port,
    ):
        super().__init__(
            scope,
            id,service_config,
            labels,
            monitoring_endpoint_port,
        )

        if (
            self.service_config.podDisruptionBudget
            and self.service_config.podDisruptionBudget.enabled
        ):
            self.pod_disruption_budget = self._create_pod_disruption_budget()

    def _create_pod_disruption_budget(self) -> k8s.KubePodDisruptionBudget:
        """Create PodDisruptionBudget resource."""
        pdb_config = self.service_config.podDisruptionBudget

        # Merge labels with common labels
        merged_labels = {**self.labels, **pdb_config.labels}

        # Build selector - use provided selector or default to pod labels
        # This ensures selector stays in sync with pod labels automatically
        selector = self._build_label_selector(
            pdb_config.selector or {}, default_match_labels=self.labels
        )

        # Build spec
        spec_kwargs = {
            "selector": selector,
        }

        # Only one of minAvailable or maxUnavailable can be set
        if pdb_config.minAvailable is not None:
            spec_kwargs["min_available"] = (
                k8s.IntOrString.from_string(str(pdb_config.minAvailable))
                if isinstance(pdb_config.minAvailable, str)
                else k8s.IntOrString.from_number(pdb_config.minAvailable)
            )
        elif pdb_config.maxUnavailable is not None:
            spec_kwargs["max_unavailable"] = (
                k8s.IntOrString.from_string(str(pdb_config.maxUnavailable))
                if isinstance(pdb_config.maxUnavailable, str)
                else k8s.IntOrString.from_number(pdb_config.maxUnavailable)
            )

        # Add unhealthyPodEvictionPolicy if specified
        if pdb_config.unhealthyPodEvictionPolicy:
            spec_kwargs["unhealthy_pod_eviction_policy"] = pdb_config.unhealthyPodEvictionPolicy

        spec = k8s.PodDisruptionBudgetSpec(**spec_kwargs)

        # Build resource name
        name = pdb_config.name if pdb_config.name else f"sequencer-{self.service_config.name}-pdb"

        return k8s.KubePodDisruptionBudget(
            self,
            "pod-disruption-budget",
            metadata=k8s.ObjectMeta(
                name=name,
                labels=merged_labels,
                annotations=pdb_config.annotations,
            ),
            spec=spec,
        )
