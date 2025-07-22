#!/usr/bin/env python
import argparse
import json
import os
from typing import Any, Dict

from cdk8s import App, Chart, Names, YamlOutputType
from constructs import Construct
from imports import k8s


def argument_parser() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--namespace", required=True, type=str, help="Required: Specify the Kubernetes namespace."
    )
    parser.add_argument(
        "--config", type=str, required=True, help="Required: Specify the sequencer config path"
    )

    return parser.parse_args()


def get_config(path: str) -> Dict[str, Any]:
    with open(os.path.abspath(path), "r") as f:
        value: Dict[str, Any] = json.loads(f.read())
        return value


class Simulator(Chart):
    def __init__(self, scope: Construct, id: str, namespace: str, config: Dict[str, Any]):
        super().__init__(scope, id, disable_resource_name_hashes=True, namespace=namespace)

        self.label = {"app": Names.to_label_value(self, include_hash=False)}
        self.config = config

        k8s.KubeConfigMap(
            self,
            "configmap",
            metadata=k8s.ObjectMeta(name=f"{self.node.id}-config"),
            data=dict(config=json.dumps(self.config)),
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
                        volumes=[
                            k8s.Volume(
                                name=f"{self.node.id}-config",
                                config_map=k8s.ConfigMapVolumeSource(name=f"{self.node.id}-config"),
                            )
                        ],
                        containers=[
                            k8s.Container(
                                name=self.node.id,
                                image="us-central1-docker.pkg.dev/starkware-dev/sequencer/simulator:0.0.1",
                                # TODO(Tsabary/Idan): this file does not exist.
                                args=["--config_file", "/config/sequencer/presets/config"],
                                env=[
                                    k8s.EnvVar(name="RUST_LOG", value="debug"),
                                    k8s.EnvVar(name="RUST_BACKTRACE", value="full"),
                                ],
                                volume_mounts=[
                                    k8s.VolumeMount(
                                        name=f"{self.node.id}-config",
                                        mount_path="/config/sequencer/presets/",
                                        read_only=True,
                                    )
                                ],
                            )
                        ],
                    ),
                ),
            ),
        )


def main() -> None:
    args = argument_parser()
    app = App(yaml_output_type=YamlOutputType.FOLDER_PER_CHART_FILE_PER_RESOURCE)
    Simulator(
        scope=app,
        id="sequencer-simulator",
        namespace=args.namespace,
        config=get_config(args.config),
    )

    app.synth()


if __name__ == "__main__":
    main()
