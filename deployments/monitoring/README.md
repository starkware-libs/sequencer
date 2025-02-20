Local monitoring stack

To run:
```bash
python3 -m venv monitoring_venv
source monitoring_venv/bin/activate
./deployments/monitoring/deploy_local_stack.sh up -d
```
This will deploy the Sequencer node, Prometheus, and Grafana containers, using the Monitoring/sequencer/dev_grafana.json dashboard for Grafana.

To shut down and clean up:
```bash
./deployments/monitoring/deploy_local_stack.sh down
deactivate
rm -rf monitoring_venv
```
