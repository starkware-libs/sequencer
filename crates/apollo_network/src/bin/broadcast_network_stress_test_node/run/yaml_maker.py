from args import get_arguments
from utils import prometheus_service_name, bootstrap_peer_id, get_prometheus_config


def get_prometheus_yaml_file(num_nodes: int):
    # Generate targets for each network stress test node (metrics on port 2000)
    urls = [
        f"network-stress-test-{i}.network-stress-test-headless:2000"
        for i in range(num_nodes)
    ]
    original_config = get_prometheus_config(self_scrape=False, metric_urls=urls)
    config = original_config.replace("\n", "\n    ")

    return f"""
apiVersion: v1
kind: ConfigMap
metadata:
  name: prometheus-config
data:
  prometheus.yml: |
    {config}
"""


def get_prometheus_deployment_yaml_file():
    return f"""
apiVersion: apps/v1
kind: Deployment
metadata:
  name: prometheus
spec:
  replicas: 1
  selector:
    matchLabels:
      app: prometheus
  template:
    metadata:
      labels:
        app: prometheus
    spec:
      containers:
      - name: prometheus
        image: registry.hub.docker.com/prom/prometheus:main
        imagePullPolicy: Always
        ports:
        - containerPort: 9090
        volumeMounts:
        - name: config-volume
          mountPath: /etc/prometheus
        args:
        - '--config.file=/etc/prometheus/prometheus.yml'
      volumes:
      - name: config-volume
        configMap:
          name: prometheus-config
"""


def get_prometheus_service_yaml_file():
    return f"""
apiVersion: v1
kind: Service
metadata:
  name: {prometheus_service_name}
spec:
  selector:
    app: prometheus
  ports:
  - port: 9090
    targetPort: 9090
  type: ClusterIP
"""


def get_network_stress_test_deployment_yaml_file(
    image_tag: str,
    args,
):
    num_nodes = args.num_nodes
    arguments = get_arguments(
        id=None,
        metric_port=2000,
        p2p_port=10000,
        bootstrap=f"/dns/network-stress-test-0.network-stress-test-headless/udp/10000/quic-v1/p2p/{bootstrap_peer_id}",
        args=args,
    )
    env_args = []
    for name, value in arguments:
        env_name = name[2:].replace("-", "_").upper()
        env_args += [f"- name: {env_name}", f'  value: "{value}"']
    for name, value in [("LATENCY", args.latency), ("THROUGHPUT", args.throughput)]:
        if value is not None:
            env_args += [f"- name: {name}", f'  value: "{value}"']

    env_vars_definitions = "\n        ".join(env_args)
    return f"""
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: network-stress-test
spec:
  serviceName: network-stress-test-headless
  replicas: {num_nodes}
  selector:
    matchLabels:
      app: network-stress-test
  template:
    metadata:
      labels:
        app: network-stress-test
    spec:
      containers:
      - name: network-stress-test
        image: {image_tag}
        securityContext:
          privileged: true
        ports:
        - containerPort: 2000
          name: metrics
        - containerPort: 10000
          protocol: UDP
          name: p2p
        env:
        {env_vars_definitions}
"""


# def get_network_stress_test_service_yaml_file():
#     return f"""
# apiVersion: v1
# kind: Service
# metadata:
#   name: network-stress-test-service
# spec:
#   selector:
#     app: network-stress-test
#   ports:
#   - port: 2000
#     targetPort: 2000
#     name: metrics
#   - port: 10000
#     targetPort: 10000
#     name: p2p
#   type: ClusterIP
# """


def get_network_stress_test_headless_service_yaml_file():
    return f"""
apiVersion: v1
kind: Service
metadata:
  name: network-stress-test-headless
spec:
  clusterIP: None
  selector:
    app: network-stress-test
  ports:
  - port: 2000
    targetPort: 2000
    name: metrics
  - port: 10000
    targetPort: 10000
    protocol: UDP
    name: p2p
"""
