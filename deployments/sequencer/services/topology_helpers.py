from services import objects, const, helpers
from config.sequencer import SequencerDevConfig


def get_pvc() -> objects.PersistentVolumeClaim:
    return objects.PersistentVolumeClaim(
        access_modes=["ReadWriteOnce"],
        storage_class_name="premium-rwo",
        volume_mode="Filesystem",
        storage="64Gi",
        mount_path="/data",
        read_only=False,
    )


def get_dev_config(config_file_path: str) -> objects.Config:
    return SequencerDevConfig(
        mount_path="/config/sequencer/presets/", config_file_path=config_file_path
    )


def get_ingress(url: str = "test.gcp-integration.sw-dev.io") -> objects.Ingress:
    return objects.Ingress(
        annotations={
            "kubernetes.io/tls-acme": "true",
            "cert-manager.io/common-name": f"{url}",
            "cert-manager.io/issue-temporary-certificate": "true",
            "cert-manager.io/issuer": "letsencrypt-prod",
            "acme.cert-manager.io/http01-edit-in-place": "true",
        },
        class_name=None,
        rules=[
            objects.IngressRule(
                host=url,
                paths=[
                    objects.IngressRuleHttpPath(
                        path="/monitoring/",
                        path_type="Prefix",
                        backend_service_name="sequencer-node-service",
                        backend_service_port_number=const.MONITORING_SERVICE_PORT,
                    )
                ],
            )
        ],
        tls=[objects.IngressTls(hosts=[url], secret_name="sequencer-tls")],
    )


def get_service() -> objects.Service:
    return objects.Service(
        type=const.ServiceType.CLUSTER_IP,
        selector={},
        ports=[
            objects.PortMapping(
                name="http",
                port=const.HTTP_SERVICE_PORT,
                container_port=const.HTTP_CONTAINER_PORT,
            ),
            objects.PortMapping(
                name="rpc",
                port=const.RPC_SERVICE_PORT,
                container_port=const.RPC_CONTAINER_PORT,
            ),
            objects.PortMapping(
                name="monitoring",
                port=const.MONITORING_SERVICE_PORT,
                container_port=const.MONITORING_CONTAINER_PORT,
            ),
        ],
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
                    objects.ContainerPort(container_port=const.HTTP_CONTAINER_PORT),
                    objects.ContainerPort(container_port=const.RPC_CONTAINER_PORT),
                    objects.ContainerPort(container_port=const.MONITORING_CONTAINER_PORT),
                ],
                startup_probe=objects.Probe(
                    port=const.MONITORING_CONTAINER_PORT,
                    path="/monitoring/nodeVersion",
                    period_seconds=10,
                    failure_threshold=10,
                    timeout_seconds=5,
                ),
                readiness_probe=objects.Probe(
                    port=const.MONITORING_CONTAINER_PORT,
                    path="/monitoring/ready",
                    period_seconds=10,
                    failure_threshold=5,
                    timeout_seconds=5,
                ),
                liveness_probe=objects.Probe(
                    port=const.MONITORING_CONTAINER_PORT,
                    path="/monitoring/alive",
                    period_seconds=10,
                    failure_threshold=5,
                    timeout_seconds=5,
                ),
                volume_mounts=[
                    objects.VolumeMount(
                        name="config",
                        mount_path="/config/sequencer/presets/",
                        read_only=True,
                    ),
                    objects.VolumeMount(name="data", mount_path="/data", read_only=False),
                ],
            )
        ],
        pvc_volumes=[objects.PvcVolume(name="data", read_only=False)],
        configmap_volumes=[objects.ConfigMapVolume(name="config")],
    )
