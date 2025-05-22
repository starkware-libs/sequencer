#!/usr/bin/env python
from cdk8s import App, Chart, Names, YamlOutputType
from constructs import Construct
from imports import k8s

SERVICE_NAME = "dummy-eth2strk-oracle"
SERVICE_PORT = 9000
CLUSTER = "sequencer-dev"
NAMESPACE = "dummy-eth2strk-oracle"
IMAGE = "us-central1-docker.pkg.dev/starkware-dev/sequencer/dummy_eth2strk_oracle:latest"


class DummyEth2StrkOracle(Chart):
    def __init__(self, scope: Construct, id: str, namespace: str):
        super().__init__(scope, id, disable_resource_name_hashes=True, namespace=namespace)

        self.label = {"app": Names.to_label_value(self, include_hash=False)}
        self.host = f"{self.node.id}.{CLUSTER}.sw-dev.io"

        k8s.KubeService(
            self,
            "service",
            spec=k8s.ServiceSpec(
                type="ClusterIP",
                ports=[
                    k8s.ServicePort(
                        name="http",
                        port=SERVICE_PORT,
                        target_port=k8s.IntOrString.from_number(SERVICE_PORT),
                    ),
                ],
                selector=self.label,
            ),
        )

        k8s.KubeDeployment(
            self,
            "deployment",
            spec=k8s.DeploymentSpec(
                replicas=1,
                selector=k8s.LabelSelector(match_labels=self.label),
                template=k8s.PodTemplateSpec(
                    metadata=k8s.ObjectMeta(labels=self.label),
                    spec=k8s.PodSpec(
                        containers=[
                            k8s.Container(
                                name=self.node.id,
                                image=IMAGE,
                                env=[k8s.EnvVar(name="RUST_LOG", value="DEBUG")],
                                ports=[k8s.ContainerPort(container_port=SERVICE_PORT)],
                            )
                        ],
                    ),
                ),
            ),
        )

        k8s.KubeIngress(
            self,
            "ingress",
            metadata=k8s.ObjectMeta(
                name=f"{self.node.id}-ingress",
                labels=self.label,
                annotations={"nginx.ingress.kubernetes.io/rewrite-target": "/"},
            ),
            spec=k8s.IngressSpec(
                ingress_class_name="nginx",
                rules=[
                    k8s.IngressRule(
                        host=self.host,
                        http=k8s.HttpIngressRuleValue(
                            paths=[
                                k8s.HttpIngressPath(
                                    path="/",
                                    path_type="ImplementationSpecific",
                                    backend=k8s.IngressBackend(
                                        service=k8s.IngressServiceBackend(
                                            name=f"{self.node.id}-service",
                                            port=k8s.ServiceBackendPort(number=SERVICE_PORT),
                                        )
                                    ),
                                )
                            ]
                        ),
                    )
                ],
            ),
        )


app = App(yaml_output_type=YamlOutputType.FOLDER_PER_CHART_FILE_PER_RESOURCE)
DummyEth2StrkOracle(scope=app, id=SERVICE_NAME, namespace=NAMESPACE)

app.synth()
