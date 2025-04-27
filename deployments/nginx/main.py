#!/usr/bin/env python

from constructs import Construct
from cdk8s import App, Chart, Names, YamlOutputType
from imports import k8s
from services import helpers

SERVICE_NAME = "nginx"
SERVICE_PORT = 8085
IMAGE = "nginx:latest"
REPLICAS = 2

PROBE_MONITORING_PATH = "/healthz"
PROBE_FAILURE_THRESHOLD = 5
PROBE_PERIOD_SECONDS = 10
PROBE_TIMEOUT_SECONDS = 5


class Nginx(Chart):
    def __init__(self, scope: Construct, id: str, namespace: str, config_file: str):
        super().__init__(scope, id, disable_resource_name_hashes=True, namespace=namespace)

        self.label = {"app": Names.to_label_value(self, include_hash=False)}
        self.config_map = k8s.KubeConfigMap(
            self,
            "configmap",
            metadata=k8s.ObjectMeta(name=f"{self.node.id}-config"),
            data={"nginx.conf": self._load_config(config_file)},
        )

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
                replicas=REPLICAS,
                selector=k8s.LabelSelector(match_labels=self.label),
                template=k8s.PodTemplateSpec(
                    metadata=k8s.ObjectMeta(labels=self.label),
                    spec=k8s.PodSpec(
                        containers=[
                            k8s.Container(
                                name=self.node.id,
                                image=IMAGE,
                                ports=[k8s.ContainerPort(container_port=SERVICE_PORT)],
                                readiness_probe=self._get_http_probe(),
                                liveness_probe=self._get_http_probe(),
                                volume_mounts=[
                                    k8s.VolumeMount(
                                        name=f"{self.node.id}-config",
                                        mount_path="/etc/nginx/nginx.conf",
                                        sub_path="nginx.conf",
                                        read_only=True,
                                    )
                                ]
                            )
                        ],
                        volumes=[
                            k8s.Volume(
                                name=f"{self.node.id}-config",
                                config_map=k8s.ConfigMapVolumeSource(name=f"{self.node.id}-config"),
                            )
                        ]
                    ),
                ),
            ),
        )
    
    def _load_config(self, path: str):
        with open(path) as f:
            return f.read()
        
    def _get_http_probe(
        self,
        period_seconds: int = PROBE_PERIOD_SECONDS,
        failure_threshold: int = PROBE_FAILURE_THRESHOLD,
        timeout_seconds: int = PROBE_TIMEOUT_SECONDS,
        path: str = PROBE_MONITORING_PATH,
    ) -> k8s.Probe:

        return k8s.Probe(
            http_get=k8s.HttpGetAction(
                path=path,
                port=k8s.IntOrString.from_number(SERVICE_PORT),
            ),
            period_seconds=period_seconds,
            failure_threshold=failure_threshold,
            timeout_seconds=timeout_seconds,
        )

args = helpers.argument_parser()
app = App(yaml_output_type=YamlOutputType.FOLDER_PER_CHART_FILE_PER_RESOURCE)
Nginx(scope=app, id=SERVICE_NAME, namespace=args.namespace, config_file=args.config_file)

app.synth()
