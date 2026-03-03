#!/usr/bin/env bash
set -e

# Repo root (directory containing .github). Run this script from repo root or it will detect it.
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
cd "$REPO_ROOT"

echo "=== Setup Python 3.10 ==="
# Use pyenv, or ensure python3.10 is on PATH; adapt if your system uses a different way.
if ! command -v python3.10 &>/dev/null; then
  echo "Python 3.10 not found. Install it (e.g. pyenv install 3.10, or system package) and ensure python3.10 is on PATH."
  exit 1
fi
python3.10 --version

echo "=== Setup Node 22 ==="
if ! command -v node &>/dev/null || [[ $(node -v | cut -d. -f1 | tr -d v) -lt 22 ]]; then
  echo "Node 22+ not found. Install it (e.g. nvm install 22, or system package) and ensure node is on PATH."
  exit 1
fi
node -v

echo "=== Install pip dependencies ==="
python3.10 -m pip install black pipenv

echo "=== Install cdk8s-cli ==="
npm install -g cdk8s-cli@2.198.334

echo "=== CDK8s synth (first overlay only) ==="
cd "$REPO_ROOT/deployments/sequencer"

echo "Importing CDK8s Sequencer app..."
cdk8s import

echo "Installing pip dependencies..."
pipenv install

echo "Synthesizing CDK8s Sequencer app with overlay hybrid.testing.node-0..."
cdk8s synth --app "pipenv run python -m main --namespace test -l hybrid -o hybrid.testing.node-0 --monitoring-dashboard-file $REPO_ROOT/deployments/monitoring/examples/output/dashboards/sequencer_node_dashboard.json --cluster test" --output dist1

echo "Done. Output in deployments/sequencer/dist1"
