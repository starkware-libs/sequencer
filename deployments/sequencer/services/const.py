from enum import Enum


# k8s service types
class K8SServiceType(str, Enum):
    CLUSTER_IP = "ClusterIP"
    LOAD_BALANCER = "LoadBalancer"
    NODE_PORT = "NodePort"


# k8s container ports
HTTP_CONTAINER_PORT = 8080
RPC_CONTAINER_PORT = 8081
MONITORING_CONTAINER_PORT = 8082

PROBE_MONITORING_READY_PATH = "/monitoring/ready"
PROBE_MONITORING_ALIVE_PATH = "/monitoring/alive"
PROBE_FAILURE_THRESHOLD = 5
PROBE_PERIOD_SECONDS = 10
PROBE_TIMEOUT_SECONDS = 5

PVC_STORAGE_CLASS_NAME = "premium-rwo"
PVC_VOLUME_MODE = "Filesystem"
PVC_ACCESS_MODE = ["ReadWriteOnce"]

HPA_MIN_REPLICAS = 1
HPA_MAX_REPLICAS = 100

SECRETS_FILE_NAME = "secrets.json"
SECRETS_MOUNT_PATH = "/etc/secrets"
