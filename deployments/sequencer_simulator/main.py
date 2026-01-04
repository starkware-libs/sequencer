#!/usr/bin/env python
import argparse
from typing import Optional

from cdk8s import App, Chart, Names, YamlOutputType
from constructs import Construct
from imports import k8s


def argument_parser():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--namespace", required=True, type=str, help="Required: Specify the Kubernetes namespace."
    )
    parser.add_argument(
        "--image", required=True, type=str, help="Required: Docker image for the simulator."
    )
    parser.add_argument(
        "--http-url",
        type=str,
        default="http://sequencer-node-service",
        help="HTTP URL for sequencer (default: http://sequencer-node-service)",
    )
    parser.add_argument(
        "--http-port", type=int, default=8080, help="HTTP port for sequencer (default: 8080)"
    )
    parser.add_argument(
        "--monitoring-url",
        type=str,
        default="http://sequencer-node-service",
        help="Monitoring URL for sequencer (default: http://sequencer-node-service)",
    )
    parser.add_argument(
        "--monitoring-port",
        type=int,
        default=8082,
        help="Monitoring port for sequencer (default: 8082)",
    )
    parser.add_argument(
        "--sender-address", type=str, required=False, help="Anvil sender address (0x...)"
    )
    parser.add_argument(
        "--receiver-address", type=str, required=False, help="Anvil receiver address (0x...)"
    )

    return parser.parse_args()


class Simulator(Chart):
    def __init__(
        self,
        scope: Construct,
        id: str,
        namespace: str,
        image: str,
        http_url: str,
        http_port: int,
        monitoring_url: str,
        monitoring_port: int,
        sender_address: Optional[str] = None,
        receiver_address: Optional[str] = None,
    ):
        super().__init__(scope, id, disable_resource_name_hashes=True, namespace=namespace)

        self.label = {"app": Names.to_label_value(self, include_hash=False)}

        # Build command arguments
        args = [
            "--http-url",
            http_url,
            "--http-port",
            str(http_port),
            "--monitoring-url",
            monitoring_url,
            "--monitoring-port",
            str(monitoring_port),
            "--run-forever",  # Run continuously
        ]

        if sender_address:
            args.extend(["--sender-address", sender_address])
        if receiver_address:
            args.extend(["--receiver-address", receiver_address])

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
                                args=args,
                                env=[
                                    k8s.EnvVar(name="RUST_LOG", value="debug"),
                                    k8s.EnvVar(name="RUST_BACKTRACE", value="full"),
                                ],
                            )
                        ],
                    ),
                ),
            ),
        )


def main():
    args = argument_parser()
    app = App(yaml_output_type=YamlOutputType.FOLDER_PER_CHART_FILE_PER_RESOURCE)
    Simulator(
        scope=app,
        id="sequencer-simulator",
        namespace=args.namespace,
        image=args.image,
        http_url=args.http_url,
        http_port=args.http_port,
        monitoring_url=args.monitoring_url,
        monitoring_port=args.monitoring_port,
        sender_address=args.sender_address,
        receiver_address=args.receiver_address,
    )

    app.synth()


if __name__ == "__main__":
    main()
