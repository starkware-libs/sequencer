from imports import k8s

from src.constructs.base import BaseConstruct


class PriorityClassConstruct(BaseConstruct):
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

        if self.service_config.priorityClass and self.service_config.priorityClass.enabled:
            # Only create PriorityClass if existingPriorityClass is not set
            if not self.service_config.priorityClass.existingPriorityClass:
                self.priority_class = self._create_priority_class()

    def _create_priority_class(self) -> k8s.KubePriorityClass:
        """Create PriorityClass resource."""
        pc_config = self.service_config.priorityClass

        # Validate that value is set when creating a new PriorityClass
        if not pc_config.value:
            raise ValueError(
                "priorityClass.value is required when creating a new PriorityClass. "
                "Set existingPriorityClass to use an existing PriorityClass instead."
            )

        # Merge labels with common labels
        merged_labels = {**self.labels, **pc_config.labels}

        # Build resource name
        name = (
            pc_config.name
            if pc_config.name
            else f"sequencer-{self.service_config.name}-priorityclass"
        )

        # PriorityClass is cluster-scoped, so no namespace
        return k8s.KubePriorityClass(
            self,
            "priority-class",
            metadata=k8s.ObjectMeta(
                name=name,
                labels=merged_labels,
                annotations=pc_config.annotations,
            ),
            value=pc_config.value,
            global_default=pc_config.globalDefault if pc_config.globalDefault else None,
            description=pc_config.description,
            preemption_policy=pc_config.preemptionPolicy,
        )
