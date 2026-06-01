## Validation-only node (Kubernetes)

Kubernetes deployment aligned with `deployments/docker/validation_node/DEPLOYMENT_GUIDE.md`:

- **validation-node** — `apollo_node` with `validation_only` config, persistent `/data`.
- **signature-manager** — same image, isolated signing; HTTP **9090** only inside the cluster (ClusterIP).
- **P2P** — consensus **53080/TCP** and state sync **53140/TCP** on a LoadBalancer service (adjust if your cluster differs).

### Prerequisites

1. **Real node config** — Replace `config/validation_node.json` with output from `deployments/docker/validation_node/setup.sh` (it writes `deployments/docker/validation_node/config/validation_node.json`; copy that file here). The checked-in file is the Docker **template** with only the numeric placeholders filled (from `environments/production.json`) so the JSON parses; **`{{...}}` string placeholders are still not substituted** and must be replaced by running `setup.sh` (or copying a fully generated config) before the node can run correctly.

2. **Signing keys** — Populate `signature-manager-secrets` (optional `envFrom` on the signature-manager pod) with whatever env vars or paths your key setup requires per Starknet docs.

3. **Firewall / LB** — Allow inbound **53080** and **53140** to the validation node service. Match `advertised_multiaddr` in the config to the public address.

### Apply

```bash
kubectl apply -k deployments/k8s/validation_node
```

### Customize

- **Namespace** — `kustomization.yaml` → `namespace:`.
- **Image** — `kustomization.yaml` → `images:`.
- **P2P service type** — `validation-node-service.yaml` (`LoadBalancer` vs `NodePort`).
- **Disk** — `validation-node-statefulset.yaml` → `volumeClaimTemplates`.

### Health checks

```bash
kubectl -n validation-node get pods
kubectl -n validation-node port-forward svc/validation-node-monitoring 8082:8082
curl http://localhost:8082/monitoring/alive
```
