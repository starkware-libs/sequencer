from typing import Optional

from constructs import Construct
from imports import k8s

from src.config.schema import CommonConfig, ServiceConfig


class BaseConstruct(Construct):
    def __init__(
        self,
        scope: Construct,
        id: str,
        common_config: Optional[CommonConfig],
        service_config: ServiceConfig,
        labels,
        monitoring_endpoint_port,
    ):
        super().__init__(scope, id)
        self.common_config = common_config
        self.service_config = service_config
        self.labels = labels
        self.monitoring_endpoint_port = monitoring_endpoint_port

    def _build_label_selector(
        self, label_selector_dict: dict, default_match_labels: dict | None = None
    ) -> k8s.LabelSelector:
        """Build Kubernetes LabelSelector from dictionary with automatic fallback to pod labels.

        This ensures labelSelector stays in sync with pod labels, preventing configuration drift.

        Args:
            label_selector_dict: Dictionary with matchLabels and/or matchExpressions
            default_match_labels: Default matchLabels to use if label_selector_dict is empty.
                                  Typically self.labels (pod labels).

        Returns:
            k8s.LabelSelector with matchLabels and/or matchExpressions
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
