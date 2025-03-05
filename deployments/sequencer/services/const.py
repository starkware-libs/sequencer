from enum import Enum

# k8s service types
class ServiceType(str, Enum):
    CLUSTER_IP = "ClusterIP"
    LOAD_BALANCER = "LoadBalancer"
    NODE_PORT = "NodePort"

IMAGE = "ghcr.io/starkware-libs/sequencer/sequencer:dev"

CONTAINER_ARGS = ["--config_file", "/config/sequencer/presets/config"]

# k8s container ports
HTTP_CONTAINER_PORT = 8080
RPC_CONTAINER_PORT = 8081
MONITORING_CONTAINER_PORT = 8082

# k8s service ports
HTTP_SERVICE_PORT = 80
GRPC_SERVICE_PORT = 8081
MONITORING_SERVICE_PORT = 8082

PROBE_FAILURE_THRESHOLD = 5
PROBE_PERIOD_SECONDS = 10
PROBE_TIMEOUT_SECONDS = 5

PVC_STORAGE_CLASS_NAME = "premium-rwo"
PVC_VOLUME_MODE = "Filesystem"
PVC_ACCESS_MODE = ["ReadWriteOnce"]
