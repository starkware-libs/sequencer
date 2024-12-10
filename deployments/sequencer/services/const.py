from enum import Enum

# k8s service types
class ServiceType(Enum):
    CLUSTER_IP = "ClusterIP"
    LOAD_BALANCER = "LoadBalancer"
    NODE_PORT = "NodePort"


# k8s container ports
HTTP_CONTAINER_PORT = 8080
RPC_CONTAINER_PORT = 8081
MONITORING_CONTAINER_PORT = 8082

# k8s service ports
HTTP_SERVICE_PORT = 80
RPC_SERVICE_PORT = 8081
MONITORING_SERVICE_PORT = 8082
