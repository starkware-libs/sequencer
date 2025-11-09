from imports import k8s
from src.constructs.base import BaseConstruct


class RbacConstruct(BaseConstruct):
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

        if self.service_config.rbac and self.service_config.rbac.enabled:
            rbac_type = self.service_config.rbac.type or "Role"

            if rbac_type == "ClusterRole":
                self.cluster_role = self._create_cluster_role()
                self.cluster_role_binding = self._create_cluster_role_binding()
            else:
                self.role = self._create_role()
                self.role_binding = self._create_role_binding()

    def _create_role(self) -> k8s.KubeRole:
        """Create Role resource."""
        rbac_config = self.service_config.rbac

        # Merge labels with common labels
        merged_labels = {**self.labels, **rbac_config.labels}

        # Build resource name
        role_name = (
            rbac_config.roleName
            if rbac_config.roleName
            else f"sequencer-{self.service_config.name}-role"
        )

        # Build rules
        rules = self._build_policy_rules(rbac_config.rules)

        return k8s.KubeRole(
            self,
            "role",
            metadata=k8s.ObjectMeta(
                name=role_name,
                labels=merged_labels,
                annotations=rbac_config.annotations,
            ),
            rules=rules,
        )

    def _create_cluster_role(self) -> k8s.KubeClusterRole:
        """Create ClusterRole resource."""
        rbac_config = self.service_config.rbac

        # Merge labels with common labels
        merged_labels = {**self.labels, **rbac_config.labels}

        # Build resource name
        role_name = (
            rbac_config.roleName
            if rbac_config.roleName
            else f"sequencer-{self.service_config.name}-clusterrole"
        )

        # Build rules
        rules = self._build_policy_rules(rbac_config.rules)

        return k8s.KubeClusterRole(
            self,
            "cluster-role",
            metadata=k8s.ObjectMeta(
                name=role_name,
                labels=merged_labels,
                annotations=rbac_config.annotations,
            ),
            rules=rules,
        )

    def _create_role_binding(self) -> k8s.KubeRoleBinding:
        """Create RoleBinding resource."""
        rbac_config = self.service_config.rbac

        # Merge labels with common labels
        merged_labels = {**self.labels, **rbac_config.labels}

        # Build resource name
        binding_name = (
            rbac_config.roleBindingName
            if rbac_config.roleBindingName
            else f"sequencer-{self.service_config.name}-rolebinding"
        )

        # Build roleRef
        role_ref = self._build_role_ref(rbac_config)

        # Build subjects
        subjects = self._build_subjects(rbac_config.subjects)

        return k8s.KubeRoleBinding(
            self,
            "role-binding",
            metadata=k8s.ObjectMeta(
                name=binding_name,
                labels=merged_labels,
                annotations=rbac_config.annotations,
            ),
            role_ref=role_ref,
            subjects=subjects,
        )

    def _create_cluster_role_binding(self) -> k8s.KubeClusterRoleBinding:
        """Create ClusterRoleBinding resource."""
        rbac_config = self.service_config.rbac

        # Merge labels with common labels
        merged_labels = {**self.labels, **rbac_config.labels}

        # Build resource name
        binding_name = (
            rbac_config.roleBindingName
            if rbac_config.roleBindingName
            else f"sequencer-{self.service_config.name}-clusterrolebinding"
        )

        # Build roleRef
        role_ref = self._build_role_ref(rbac_config, cluster_role=True)

        # Build subjects
        subjects = self._build_subjects(rbac_config.subjects)

        return k8s.KubeClusterRoleBinding(
            self,
            "cluster-role-binding",
            metadata=k8s.ObjectMeta(
                name=binding_name,
                labels=merged_labels,
                annotations=rbac_config.annotations,
            ),
            role_ref=role_ref,
            subjects=subjects,
        )

    def _build_policy_rules(self, rules: list) -> list[k8s.PolicyRule]:
        """Convert rule dictionaries to k8s.PolicyRule objects."""
        policy_rules = []

        for rule in rules:
            rule_kwargs = {}

            # Required: apiGroups
            if "apiGroups" in rule:
                rule_kwargs["api_groups"] = rule["apiGroups"]
            else:
                rule_kwargs["api_groups"] = [""]  # Core API group

            # Required: resources
            if "resources" in rule:
                rule_kwargs["resources"] = rule["resources"]
            else:
                rule_kwargs["resources"] = []

            # Required: verbs
            if "verbs" in rule:
                rule_kwargs["verbs"] = rule["verbs"]
            else:
                rule_kwargs["verbs"] = []

            # Optional: resourceNames
            if "resourceNames" in rule:
                rule_kwargs["resource_names"] = rule["resourceNames"]

            # Optional: nonResourceURLs (for ClusterRole only)
            if "nonResourceURLs" in rule:
                rule_kwargs["non_resource_urls"] = rule["nonResourceURLs"]

            policy_rules.append(k8s.PolicyRule(**rule_kwargs))

        return policy_rules

    def _build_role_ref(self, rbac_config, cluster_role: bool = False) -> k8s.RoleRef:
        """Build RoleRef object."""
        # If custom roleRef is provided, use it; otherwise auto-generate
        if rbac_config.roleRef:
            role_ref_dict = rbac_config.roleRef
            return k8s.RoleRef(
                api_group=role_ref_dict.get("apiGroup", "rbac.authorization.k8s.io"),
                kind=role_ref_dict.get("kind", "ClusterRole" if cluster_role else "Role"),
                name=role_ref_dict.get("name"),
            )

        # Auto-generate roleRef
        role_name = (
            rbac_config.roleName
            if rbac_config.roleName
            else f"sequencer-{self.service_config.name}-{'clusterrole' if cluster_role else 'role'}"
        )

        return k8s.RoleRef(
            api_group="rbac.authorization.k8s.io",
            kind="ClusterRole" if cluster_role else "Role",
            name=role_name,
        )

    def _build_subjects(self, subjects: list) -> list[k8s.Subject]:
        """Convert subject dictionaries to k8s.Subject objects."""
        # If no subjects provided, default to service account
        if not subjects:
            sa_name = (
                self.service_config.serviceAccount.name
                if self.service_config.serviceAccount and self.service_config.serviceAccount.name
                else f"sequencer-{self.service_config.name}-sa"
            )
            from cdk8s import Chart

            chart = self._find_chart_parent()
            namespace = chart.namespace if chart and hasattr(chart, "namespace") else None

            return [
                k8s.Subject(
                    kind="ServiceAccount",
                    name=sa_name,
                    namespace=namespace,
                )
            ]

        subject_objects = []

        for subject in subjects:
            subject_kwargs = {}

            # Required: kind
            if "kind" in subject:
                subject_kwargs["kind"] = subject["kind"]
            else:
                subject_kwargs["kind"] = "ServiceAccount"  # Default

            # Required: name
            subject_name = subject.get("name", "")
            if subject_name:
                subject_kwargs["name"] = subject_name
            else:
                # Default to service account name if kind is ServiceAccount
                if subject.get("kind") == "ServiceAccount" or "kind" not in subject:
                    sa_name = (
                        self.service_config.serviceAccount.name
                        if self.service_config.serviceAccount
                        and self.service_config.serviceAccount.name
                        else f"sequencer-{self.service_config.name}-sa"
                    )
                    subject_kwargs["name"] = sa_name
                else:
                    raise ValueError("Subject 'name' is required when kind is not ServiceAccount")

            # Optional: namespace (required for ServiceAccount)
            if "namespace" in subject:
                subject_kwargs["namespace"] = subject["namespace"]
            elif subject.get("kind") == "ServiceAccount" or "kind" not in subject:
                # Default to chart namespace for ServiceAccount
                # Get namespace from the Chart scope
                from cdk8s import Chart

                chart = self._find_chart_parent()
                if chart and hasattr(chart, "namespace"):
                    subject_kwargs["namespace"] = chart.namespace

            # Optional: apiGroup
            if "apiGroup" in subject:
                subject_kwargs["api_group"] = subject["apiGroup"]

            subject_objects.append(k8s.Subject(**subject_kwargs))

        return subject_objects

    def _find_chart_parent(self):
        """Find the Chart parent in the construct tree."""
        from cdk8s import Chart

        current = self
        for _ in range(10):  # Limit depth to avoid infinite loops
            if isinstance(current, Chart):
                return current
            if hasattr(current, "node") and hasattr(current.node, "scope"):
                current = current.node.scope
            elif hasattr(current, "scope"):
                current = current.scope
            else:
                break
        return None
