#!/usr/bin/env python

import argparse
from typing import Optional

from cdk8s import App, Chart, Names, YamlOutputType
from constructs import Construct
from imports import k8s

SERVICE_PORT = 9000
DEFAULT_IMAGE = "us-central1-docker.pkg.dev/starkware-dev/sequencer/dummy_eth2strk_oracle:latest"


def argument_parser() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--namespace", required=True, type=str, help="Kubernetes namespace.")
    parser.add_argument(
        "--image",
        type=str,
        default=DEFAULT_IMAGE,
        help="Docker image to use (default: %(default)s)",
    )
    parser.add_argument(
        "--create-ingress",
        default=False,
        action="store_true",
        help="Enable ingress.",
    )
    parser.add_argument(
        "--cluster",
        type=str,
        help="Kubernetes cluster name. Required if --create-ingress is used.",
    )
    parser.add_argument(
        "--ingress-domain",
        type=str,
        help="Ingress domain. Required if --create-ingress is used.",
    )

    args = parser.parse_args()
    assert not args.create_ingress or (
        args.cluster and args.ingress_domain
    ), "--cluster and --ingress-domain are required if --create-ingress is used."

    return args


class DummyEth2StrkOracle(Chart):
    def __init__(
        self,
        scope: Construct,
        id: str,
        namespace: str,
        image: str,
        create_ingress: bool,
        cluster: Optional[str],
        domain: Optional[str],
    ):
        super().__init__(scope, id, disable_resource_name_hashes=True, namespace=namespace)

        self.label = {"app": Names.to_label_value(self, include_hash=False)}
        self.cluster = cluster
        self.create_ingress = create_ingress
        self.domain = domain
        self.image = image

        self._get_service()
        self._get_deployment()
        if self.create_ingress:
            self._get_ingress()

    def _get_service(self) -> k8s.KubeService:
        return k8s.KubeService(
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

    def _get_deployment(self) -> k8s.KubeDeployment:
        return k8s.KubeDeployment(
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
                                image=self.image,
                                env=[k8s.EnvVar(name="RUST_LOG", value="DEBUG")],
                                ports=[k8s.ContainerPort(container_port=SERVICE_PORT)],
                            )
                        ],
                    ),
                ),
            ),
        )

    def _get_ingress(self) -> k8s.KubeIngress:
        host = f"{self.node.id}.{self.cluster}.{self.domain}"
        return k8s.KubeIngress(
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
                        host=host,
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


def main() -> None:
    args = argument_parser()
    app = App(yaml_output_type=YamlOutputType.FOLDER_PER_CHART_FILE_PER_RESOURCE)

    DummyEth2StrkOracle(
        scope=app,
        id="dummy-eth2strk-oracle",
        namespace=args.namespace,
        image=args.image,
        cluster=args.cluster,
        domain=args.ingress_domain,
        create_ingress=args.create_ingress,
    )

    app.synth()


if __name__ == "__main__":
    main()
