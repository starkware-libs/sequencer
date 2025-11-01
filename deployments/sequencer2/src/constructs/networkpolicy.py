from imports import k8s

from src.constructs.base import BaseConstruct


class NetworkPolicyConstruct(BaseConstruct):
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

        if self.service_config.networkPolicy and self.service_config.networkPolicy.enabled:
            self.network_policy = self._create_network_policy()

    def _create_network_policy(self) -> k8s.KubeNetworkPolicy:
        """Create NetworkPolicy resource."""
        np_config = self.service_config.networkPolicy

        # Merge labels with common labels
        merged_labels = {**self.labels, **np_config.labels}

        # Build pod selector - use provided selector or default to service labels
        pod_selector_dict = np_config.podSelector or {}

        # Extract matchLabels and matchExpressions
        match_labels = pod_selector_dict.get("matchLabels", {})
        if not match_labels:
            # Default to service labels if no matchLabels specified
            match_labels = self.labels

        match_expressions = pod_selector_dict.get("matchExpressions", [])

        # Convert matchExpressions to k8s.LabelSelectorRequirement if provided
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

        pod_selector = k8s.LabelSelector(
            match_labels=match_labels,
            match_expressions=label_selector_requirements,
        )

        # Build spec
        spec_kwargs = {
            "pod_selector": pod_selector,
        }

        # Build ingress rules
        ingress_rules = []
        for ingress_rule in np_config.ingress:
            ingress_rule_kwargs = {}

            # Handle ports
            if "ports" in ingress_rule:
                ports = []
                for port in ingress_rule["ports"]:
                    port_kwargs = {}
                    if "protocol" in port:
                        port_kwargs["protocol"] = port["protocol"]
                    if "port" in port:
                        # Port can be int or string
                        port_value = port["port"]
                        if isinstance(port_value, str):
                            port_kwargs["port"] = k8s.IntOrString.from_string(port_value)
                        else:
                            port_kwargs["port"] = k8s.IntOrString.from_number(port_value)
                    if port_kwargs:
                        ports.append(k8s.NetworkPolicyPort(**port_kwargs))
                if ports:
                    ingress_rule_kwargs["ports"] = ports

            # Handle from (NetworkPolicyPeer)
            if "from" in ingress_rule:
                from_peers = []
                for peer in ingress_rule["from"]:
                    peer_kwargs = {}
                    if "podSelector" in peer:
                        peer_pod_selector = peer["podSelector"]
                        peer_match_labels = peer_pod_selector.get("matchLabels", {})
                        peer_match_expressions = peer_pod_selector.get("matchExpressions", [])

                        peer_label_requirements = None
                        if peer_match_expressions:
                            peer_label_requirements = [
                                k8s.LabelSelectorRequirement(
                                    key=expr.get("key"),
                                    operator=expr.get("operator"),
                                    values=expr.get("values", []),
                                )
                                for expr in peer_match_expressions
                            ]

                        peer_kwargs["pod_selector"] = k8s.LabelSelector(
                            match_labels=peer_match_labels,
                            match_expressions=peer_label_requirements,
                        )
                    if "namespaceSelector" in peer:
                        ns_selector = peer["namespaceSelector"]
                        ns_match_labels = ns_selector.get("matchLabels", {})
                        ns_match_expressions = ns_selector.get("matchExpressions", [])

                        ns_label_requirements = None
                        if ns_match_expressions:
                            ns_label_requirements = [
                                k8s.LabelSelectorRequirement(
                                    key=expr.get("key"),
                                    operator=expr.get("operator"),
                                    values=expr.get("values", []),
                                )
                                for expr in ns_match_expressions
                            ]

                        peer_kwargs["namespace_selector"] = k8s.LabelSelector(
                            match_labels=ns_match_labels,
                            match_expressions=ns_label_requirements,
                        )
                    if "ipBlock" in peer:
                        ip_block = peer["ipBlock"]
                        peer_kwargs["ip_block"] = k8s.IpBlock(
                            cidr=ip_block.get("cidr"),
                            except_=ip_block.get("except", []),
                        )
                    if peer_kwargs:
                        from_peers.append(k8s.NetworkPolicyPeer(**peer_kwargs))
                if from_peers:
                    ingress_rule_kwargs["from_"] = from_peers

            if ingress_rule_kwargs:
                ingress_rules.append(k8s.NetworkPolicyIngressRule(**ingress_rule_kwargs))

        if ingress_rules:
            spec_kwargs["ingress"] = ingress_rules

        # Build egress rules
        egress_rules = []
        for egress_rule in np_config.egress:
            egress_rule_kwargs = {}

            # Handle ports
            if "ports" in egress_rule:
                ports = []
                for port in egress_rule["ports"]:
                    port_kwargs = {}
                    if "protocol" in port:
                        port_kwargs["protocol"] = port["protocol"]
                    if "port" in port:
                        port_value = port["port"]
                        if isinstance(port_value, str):
                            port_kwargs["port"] = k8s.IntOrString.from_string(port_value)
                        else:
                            port_kwargs["port"] = k8s.IntOrString.from_number(port_value)
                    if port_kwargs:
                        ports.append(k8s.NetworkPolicyPort(**port_kwargs))
                if ports:
                    egress_rule_kwargs["ports"] = ports

            # Handle to (NetworkPolicyPeer)
            if "to" in egress_rule:
                to_peers = []
                for peer in egress_rule["to"]:
                    peer_kwargs = {}
                    if "podSelector" in peer:
                        peer_pod_selector = peer["podSelector"]
                        peer_match_labels = peer_pod_selector.get("matchLabels", {})
                        peer_match_expressions = peer_pod_selector.get("matchExpressions", [])

                        peer_label_requirements = None
                        if peer_match_expressions:
                            peer_label_requirements = [
                                k8s.LabelSelectorRequirement(
                                    key=expr.get("key"),
                                    operator=expr.get("operator"),
                                    values=expr.get("values", []),
                                )
                                for expr in peer_match_expressions
                            ]

                        peer_kwargs["pod_selector"] = k8s.LabelSelector(
                            match_labels=peer_match_labels,
                            match_expressions=peer_label_requirements,
                        )
                    if "namespaceSelector" in peer:
                        ns_selector = peer["namespaceSelector"]
                        ns_match_labels = ns_selector.get("matchLabels", {})
                        ns_match_expressions = ns_selector.get("matchExpressions", [])

                        ns_label_requirements = None
                        if ns_match_expressions:
                            ns_label_requirements = [
                                k8s.LabelSelectorRequirement(
                                    key=expr.get("key"),
                                    operator=expr.get("operator"),
                                    values=expr.get("values", []),
                                )
                                for expr in ns_match_expressions
                            ]

                        peer_kwargs["namespace_selector"] = k8s.LabelSelector(
                            match_labels=ns_match_labels,
                            match_expressions=ns_label_requirements,
                        )
                    if "ipBlock" in peer:
                        ip_block = peer["ipBlock"]
                        peer_kwargs["ip_block"] = k8s.IpBlock(
                            cidr=ip_block.get("cidr"),
                            except_=ip_block.get("except", []),
                        )
                    if peer_kwargs:
                        to_peers.append(k8s.NetworkPolicyPeer(**peer_kwargs))
                if to_peers:
                    egress_rule_kwargs["to"] = to_peers

            if egress_rule_kwargs:
                egress_rules.append(k8s.NetworkPolicyEgressRule(**egress_rule_kwargs))

        if egress_rules:
            spec_kwargs["egress"] = egress_rules

        # Auto-detect policyTypes if not explicitly specified
        # If ingress rules exist, add "Ingress"; if egress rules exist, add "Egress"
        # If both arrays are empty but policyTypes is specified, use it
        # If policyTypes is empty and no rules, Kubernetes defaults to both
        if np_config.policyTypes:
            spec_kwargs["policy_types"] = np_config.policyTypes
        elif ingress_rules or egress_rules:
            # Auto-detect based on which rules exist
            policy_types = []
            if ingress_rules:
                policy_types.append("Ingress")
            if egress_rules:
                policy_types.append("Egress")
            if policy_types:
                spec_kwargs["policy_types"] = policy_types

        spec = k8s.NetworkPolicySpec(**spec_kwargs)

        # Build resource name
        name = (
            np_config.name
            if np_config.name
            else f"sequencer-{self.service_config.name}-networkpolicy"
        )

        return k8s.KubeNetworkPolicy(
            self,
            "network-policy",
            metadata=k8s.ObjectMeta(
                name=name,
                labels=merged_labels,
                annotations=np_config.annotations,
            ),
            spec=spec,
        )
