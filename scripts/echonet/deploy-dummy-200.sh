#!/usr/bin/env bash
set -euo pipefail

# Wrapper to deploy dummy-200 via Kustomize
# Options:
#   -x  delete existing resources first (kubectl delete -k)
#   -l  allow files outside kustomize dir (use --load-restrictor=LoadRestrictionsNone)
#   -r  rollout restart deployment after apply (to pick up code changes)
#   -n  namespace (kubectl -n <ns>)

DELETE_FIRST=false
ALLOW_OUTSIDE=false
ROLL_RESTART=false
NAMESPACE=""

while getopts ":xlrn:" opt; do
  case "$opt" in
    x) DELETE_FIRST=true ;;
    l) ALLOW_OUTSIDE=true ;;
    r) ROLL_RESTART=true ;;
    n) NAMESPACE="-n ${OPTARG}" ;;
    :) echo "Option -$OPTARG requires an argument." >&2; exit 2 ;;
    \?) echo "Unknown option -$OPTARG" >&2; exit 2 ;;
  esac
done
SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
KUSTOMIZE_DIR="$SCRIPT_DIR/k8s/dummy-200"

if $DELETE_FIRST; then
  echo "[deploy] Deleting existing resources..."
  kubectl $NAMESPACE delete -k "$KUSTOMIZE_DIR" --ignore-not-found
fi

echo "[deploy] Applying manifests..."
if $ALLOW_OUTSIDE; then
  # Use kustomize render with relaxed load restrictor, then apply
  kubectl kustomize "$KUSTOMIZE_DIR" --load-restrictor=LoadRestrictionsNone | kubectl $NAMESPACE apply -f -
else
  kubectl $NAMESPACE apply -k "$KUSTOMIZE_DIR"
fi

if $ROLL_RESTART; then
  echo "[deploy] Rolling restart deployment/dummy-200..."
  kubectl $NAMESPACE rollout restart deployment/dummy-200
  kubectl $NAMESPACE rollout status deployment/dummy-200
fi

echo "[deploy] Done."



