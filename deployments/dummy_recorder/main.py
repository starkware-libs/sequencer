#!/usr/bin/env python
<<<<<<< HEAD

import argparse
from typing import Optional
from constructs import Construct
||||||| 92f8b4a29
from constructs import Construct
=======

import argparse
>>>>>>> origin/main-v0.14.0
from cdk8s import App, Chart, Names, YamlOutputType
from constructs import Construct
from imports import k8s

SERVICE_PORT = 8080
IMAGE = "ghcr.io/starkware-libs/sequencer/dummy_recorder:latest"


def argument_parser():
    parser = argparse.ArgumentParser()
    parser.add_argument("--namespace", required=True, type=str, help="Kubernetes namespace.")
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


def get_args():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--namespace",
        required=True,
        help="Kubernetes namespace to deploy to."
    )
    parser.add_argument(
        "--image",
        required=False,
        default="us-central1-docker.pkg.dev/starkware-dev/sequencer/dummy_recorder:latest",
        help="Docker image to deploy. Defaults to the latest dummy recorder image."
    )
    return parser.parse_args()


class DummyRecorder(Chart):
<<<<<<< HEAD
    def __init__(
        self,
        scope: Construct,
        id: str,
        namespace: str,
        create_ingress: bool,
        cluster: Optional[str],
        domain: Optional[str],
    ):
||||||| 92f8b4a29
    def __init__(self, scope: Construct, id: str, namespace: str):
=======
    def __init__(self, scope: Construct, id: str, namespace: str, image: str):
>>>>>>> origin/main-v0.14.0
        super().__init__(scope, id, disable_resource_name_hashes=True, namespace=namespace)

        self.label = {"app": Names.to_label_value(self, include_hash=False)}
<<<<<<< HEAD
        self.cluster = cluster
        self.create_ingress = create_ingress
        self.domain = domain
||||||| 92f8b4a29
        self.host = f"{self.node.id}.{self.namespace}.sw-dev.io"
        
        k8s.KubeService(
          self,
          "service",
          spec=k8s.ServiceSpec(
              type="ClusterIP",
              ports=[
                k8s.ServicePort(
                  name="http",
                  port=8080,
                  target_port=k8s.IntOrString.from_number(8080)
                ),
              ],
              selector=self.label,
          ),
        )
=======
        self.host = f"{self.node.id}.{self.namespace}.sw-dev.io"

        k8s.KubeService(
            self,
            "service",
            spec=k8s.ServiceSpec(
                type="ClusterIP",
                ports=[
                    k8s.ServicePort(
                        name="http", port=8080, target_port=k8s.IntOrString.from_number(8080)
                    ),
                ],
                selector=self.label,
            ),
        )
>>>>>>> origin/main-v0.14.0

<<<<<<< HEAD
        self._get_service()
        self._get_deployment()
        if self.create_ingress:
            self._get_ingress()

    def _get_service(self):
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
||||||| 92f8b4a29
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
                    image="us-central1-docker.pkg.dev/starkware-dev/sequencer/dummy_recorder:latest",
                    env=[
                      k8s.EnvVar(
                        name="RUST_LOG",
                        value="DEBUG"
                      )
                    ],
                    ports=[
                      k8s.ContainerPort(
                        container_port=8080
                      )
                    ],
                  )
                ],
              ),
=======
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
                                image=image,
                                env=[k8s.EnvVar(name="RUST_LOG", value="DEBUG")],
                                ports=[k8s.ContainerPort(container_port=8080)],
                            )
                        ],
                    ),
                ),
>>>>>>> origin/main-v0.14.0
            ),
        )

<<<<<<< HEAD
    def _get_deployment(self):
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
                                image=IMAGE,
                                env=[k8s.EnvVar(name="RUST_LOG", value="DEBUG")],
                                ports=[k8s.ContainerPort(container_port=SERVICE_PORT)],
                            )
                        ],
                    ),
                ),
            ),
        )

    def _get_ingress(self):
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
||||||| 92f8b4a29
        k8s.KubeIngress(
          self,
          "ingress",
          metadata=k8s.ObjectMeta(
            name=f"{self.node.id}-ingress",
            labels=self.label,
            annotations={
              "nginx.ingress.kubernetes.io/rewrite-target": "/"
            }
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
                          port=k8s.ServiceBackendPort(
                              number=8080
                          ),
                        )
                      ),
                    )
                  ]
                ),
              )
            ]
          ),
=======
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
                                            port=k8s.ServiceBackendPort(number=8080),
                                        )
                                    ),
                                )
                            ]
                        ),
                    )
                ],
            ),
>>>>>>> origin/main-v0.14.0
        )


<<<<<<< HEAD
def main():
    args = argument_parser()
    app = App(yaml_output_type=YamlOutputType.FOLDER_PER_CHART_FILE_PER_RESOURCE)
||||||| 92f8b4a29
app = App(yaml_output_type=YamlOutputType.FOLDER_PER_CHART_FILE_PER_RESOURCE)
DummyRecorder(
  scope=app, 
  id="dummy-recorder",
  namespace="dummy-recorder"
)
=======
if __name__ == "__main__":
    args = get_args()
>>>>>>> origin/main-v0.14.0

<<<<<<< HEAD
    DummyRecorder(
        scope=app,
        id="dummy-recorder",
        namespace=args.namespace,
        cluster=args.cluster,
        domain=args.ingress_domain,
        create_ingress=args.create_ingress,
    )

    app.synth()


if __name__ == "__main__":
    main()
||||||| 92f8b4a29
app.synth()
=======
    app = App(yaml_output_type=YamlOutputType.FOLDER_PER_CHART_FILE_PER_RESOURCE)
    DummyRecorder(
        scope=app,
        id="dummy-recorder",
        namespace=args.namespace,
        image=args.image
    )
    app.synth()
>>>>>>> origin/main-v0.14.0
