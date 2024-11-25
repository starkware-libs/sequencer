import typing

from services import objects
from config.sequencer import SequencerDevConfig
from services.helpers import args


def get_pvc() -> objects.PersistentVolumeClaim:
  return objects.PersistentVolumeClaim(
      access_modes=["ReadWriteOnce"],
      storage_class_name="premium-rwo",
      volume_mode="Filesystem",
      storage="64Gi",
      mount_path="/data",
      read_only=False
    )


def get_port_mappings() -> typing.Sequence[objects.PortMapping]:
  return [
      objects.PortMapping(name="http", port=80, container_port=8080),
      objects.PortMapping(name="rpc", port=8081, container_port=8081),
      objects.PortMapping(name="monitoring", port=8082, container_port=8082)
  ]


def get_config() -> objects.Config:
  return SequencerDevConfig(
    mount_path="/config/sequencer/presets/", 
    config_file_path=args.config_file
  )


def get_ingress() -> objects.Ingress:
  return objects.Ingress(
        annotations={
            "kubernetes.io/tls-acme": "true",
            "cert-manager.io/common-name": f"{args.namespace}.gcp-integration.sw-dev.io",
            "cert-manager.io/issue-temporary-certificate": "true",
            "cert-manager.io/issuer": "letsencrypt-prod",
            "acme.cert-manager.io/http01-edit-in-place": "true"
        },
        class_name=None,
        rules=[
            objects.IngressRule(
                host=f"{args.namespace}.gcp-integration.sw-dev.io",
                paths=[
                    objects.IngressRuleHttpPath(
                        path="/monitoring/",
                        path_type="Prefix",
                        backend_service_name="sequencer-node-service",
                        backend_service_port_number=8082
                    )
                ]
            )
        ],
        tls=[
            objects.IngressTls(
                hosts=[
                    f"{args.namespace}.gcp-integration.sw-dev.io"
                ],
                secret_name="sequencer-tls"
            )
        ]
    )


def get_service() -> objects.Service:
  return objects.Service(
    type=objects.ServiceType.CLUSTER_IP.value,
    selector={},
    ports=[
      objects.PortMapping(
        name="http",
        port=80,
        container_port=8080
      ),
      objects.PortMapping(
        name="rpc",
        port=8081,
        container_port=8081
      ),
      objects.PortMapping(
        name="monitoring",
        port=8082,
        container_port=8082
      )
    ]
  )


def get_deployment() -> objects.Deployment:
  return objects.Deployment(
    replicas=1,
    annotations={},
    containers=[
      objects.Container(
        name="server",
        image="us.gcr.io/starkware-dev/sequencer-node-test:0.0.1-dev.3",
        args=["--config_file", "/config/sequencer/presets/config"],
        ports=[
          objects.ContainerPort(container_port=8080),
          objects.ContainerPort(container_port=8081),
          objects.ContainerPort(container_port=8082)
        ],
        startup_probe=objects.Probe(port=8082, path="/monitoring/nodeVersion", period_seconds=10, failure_threshold=10, timeout_seconds=5),
        readiness_probe=objects.Probe(port=8082, path="/monitoring/ready", period_seconds=10, failure_threshold=5, timeout_seconds=5),
        liveness_probe=objects.Probe(port=8082, path="/monitoring/alive", period_seconds=10, failure_threshold=5, timeout_seconds=5),
        volume_mounts=[
          objects.VolumeMount(
            name="config",
            mount_path="/config/sequencer/presets/",
            read_only=True
          ),
          objects.VolumeMount(
            name="data",
            mount_path="/data",
            read_only=False
          )
        ]
      )
    ],
    pvc_volumes=[
      objects.PvcVolume(
        name="data",
        read_only=False
      )
    ],
    configmap_volumes=[
      objects.ConfigMapVolume(
        name="config"
      )
    ]
  )
