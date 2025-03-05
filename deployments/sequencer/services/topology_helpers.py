import typing

from services import objects, const
from config.sequencer import SequencerDevConfig


def get_dev_config(config_file_path: str) -> objects.Config:
    return SequencerDevConfig(
        config_file_path=config_file_path
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
                        path="/monitoring",
                        path_type="Prefix",
                        backend_service_name="sequencer-node-service",
                        backend_service_port_number=const.MONITORING_SERVICE_PORT,
                    )
                ],
            )
        ],
        tls=[objects.IngressTls(hosts=[url], secret_name="sequencer-tls")],
    )


