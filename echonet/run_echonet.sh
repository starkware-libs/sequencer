#!/usr/bin/env bash
set -euo pipefail

# ---- Configurable defaults ----
NAMESPACE=""
CONFIGMAP_NAME="sequencer-node-config"
STATEFULSET_NAME="sequencer-node-statefulset"
OUTFILE="sequencer-node-config.yaml"
DRYRUN="false"
WAIT_TIMEOUT_SECS=900   # 15 minutes for the log wait
LOG_LINE="Starting eternal pending."
POD_ORDINAL="0"         # pod index: ${STATEFULSET_NAME}-0

usage() {
  cat <<EOF
Toggle "revert_config.should_revert" in a ConfigMap and restart the statefulset.

Usage:
  $(basename "$0") [--namespace NAMESPACE] [--configmap NAME] [--statefulset NAME] [--outfile PATH] [--dry-run]

Options:
  -n, --namespace     Kubernetes namespace (uses current context if omitted)
  -c, --configmap     ConfigMap name (default: ${CONFIGMAP_NAME})
  -s, --statefulset   StatefulSet name (default: ${STATEFULSET_NAME})
  -o, --outfile       Backup YAML path (default: ${OUTFILE})
      --dry-run       Preview only; no apply/restart/log-wait/port-forward
  -h, --help          Show help
EOF
}

# ---- Parse args ----
while [[ $# -gt 0 ]]; do
  case "$1" in
    -n|--namespace)    NAMESPACE="$2"; shift 2;;
    -c|--configmap)    CONFIGMAP_NAME="$2"; shift 2;;
    -s|--statefulset)  STATEFULSET_NAME="$2"; shift 2;;
    -o|--outfile)      OUTFILE="$2"; shift 2;;
    --dry-run)         DRYRUN="true"; shift;;
    -h|--help)         usage; exit 0;;
    *) echo "Unknown option: $1"; usage; exit 1;;
  esac
done

NS_ARGS=()
[[ -n "$NAMESPACE" ]] && NS_ARGS=( -n "$NAMESPACE" )

# ---- Requirements ----
for cmd in kubectl jq; do
  command -v "$cmd" >/dev/null || { echo "Error: '$cmd' is required."; exit 1; }
done

tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT

echo "Fetching ConfigMap '${CONFIGMAP_NAME}' ${NAMESPACE:+in namespace '$NAMESPACE'}..."
kubectl get configmap "$CONFIGMAP_NAME" "${NS_ARGS[@]}" -o yaml > "$OUTFILE"
kubectl get configmap "$CONFIGMAP_NAME" "${NS_ARGS[@]}" -o json > "$tmp/cm.json"

# Extract the JSON string under .data.config
CONFIG_JSON_STR="$(jq -r '.data.config' "$tmp/cm.json")" || true
if [[ -z "$CONFIG_JSON_STR" || "$CONFIG_JSON_STR" == "null" ]]; then
  echo "Error: .data.config not found in ConfigMap."
  exit 1
fi

# Build a PRETTY-PRINTED config JSON for 'true' phase
printf '%s' "$CONFIG_JSON_STR" \
  | jq '."revert_config.should_revert" = true | .' \
  | jq -S '.' > "$tmp/pretty_true.json"

# Prepare updated ConfigMap JSON for true phase
jq --rawfile cfg "$tmp/pretty_true.json" '.data.config = $cfg' "$tmp/cm.json"  > "$tmp/cm.true.updated.json"

# Emit human-friendly updated YAML preview (true)
echo "Writing updated YAML preview (true) to: ${OUTFILE%.yaml}.true.updated.yaml"
kubectl apply --dry-run=client -f "$tmp/cm.true.updated.json" -o yaml > "${OUTFILE%.yaml}.true.updated.yaml"

if [[ "$DRYRUN" == "true" ]]; then
  # Also preview the eventual 'false' state (based on the initial fetch)
  printf '%s' "$CONFIG_JSON_STR" \
    | jq '."revert_config.should_revert" = false | .' \
    | jq -S '.' > "$tmp/pretty_false.dry.json"
  jq --rawfile cfg "$tmp/pretty_false.dry.json" '.data.config = $cfg' "$tmp/cm.json" > "$tmp/cm.false.updated.dry.json"
  echo "Writing updated YAML preview (false) to: ${OUTFILE%.yaml}.false.updated.yaml"
  kubectl apply --dry-run=client -f "$tmp/cm.false.updated.dry.json" -o yaml > "${OUTFILE%.yaml}.false.updated.yaml"
  echo "Dry-run only. Not applying or restarting."
  exit 0
fi

# PHASE 1: Apply 'true', restart, and wait for "Starting eternal pending"
echo "Applying updated ConfigMap (should_revert=true)..."
kubectl apply -f "$tmp/cm.true.updated.json" "${NS_ARGS[@]}"

echo "Restarting StatefulSet '${STATEFULSET_NAME}' (phase 1)..."
kubectl scale statefulset "$STATEFULSET_NAME" --replicas=0 "${NS_ARGS[@]}"
sleep 3
kubectl scale statefulset "$STATEFULSET_NAME" --replicas=1 "${NS_ARGS[@]}"

POD_NAME="${STATEFULSET_NAME}-${POD_ORDINAL}"
echo "Waiting for pod/${POD_NAME} to be Ready (phase 1)..."
kubectl wait --for=condition=ready "pod/${POD_NAME}" --timeout=300s "${NS_ARGS[@]}"

# Matches JSON or plain text, with or without trailing period.
LOG_REGEX='("message":"Starting eternal pending\.?"|Starting eternal pending\.?)'
echo "should_revert=true → streaming logs for: Starting eternal pending (timeout: ${WAIT_TIMEOUT_SECS}s)"

CONTAINER_ARGS=()
if [[ -n "${CONTAINER:-}" ]]; then CONTAINER_ARGS=(-c "$CONTAINER"); fi

if command -v timeout >/dev/null 2>&1; then
  if timeout "${WAIT_TIMEOUT_SECS}s" \
       grep -m1 -E "$LOG_REGEX" < <(kubectl logs "pod/${POD_NAME}" "${NS_ARGS[@]}" "${CONTAINER_ARGS[@]}" --since=30m -f)
  then
    echo "✔ Detected: Starting eternal pending"
  else
    echo "✖ Timeout after ${WAIT_TIMEOUT_SECS}s waiting for 'Starting eternal pending'."
    exit 1
  fi
else
  grep -m1 -E "$LOG_REGEX" < <(kubectl logs "pod/${POD_NAME}" "${NS_ARGS[@]}" "${CONTAINER_ARGS[@]}" --since=30m -f) &
  watcher_pid=$!
  waited=0
  while kill -0 "$watcher_pid" 2>/dev/null && (( waited < WAIT_TIMEOUT_SECS )); do
    sleep 1; (( waited++ ))
  done
  if kill -0 "$watcher_pid" 2>/dev/null; then
    kill "$watcher_pid" 2>/dev/null || true
    echo "✖ Timeout after ${WAIT_TIMEOUT_SECS}s waiting for 'Starting eternal pending'."
    exit 1
  else
    echo "✔ Detected: Starting eternal pending"
  fi
fi

# PHASE 2: Re-fetch, set 'false', redeploy echonet, restart, and wait
echo "Fetching ConfigMap '${CONFIGMAP_NAME}' again for false phase..."
kubectl get configmap "$CONFIGMAP_NAME" "${NS_ARGS[@]}" -o json > "$tmp/cm2.json"
CONFIG_JSON_STR_2="$(jq -r '.data.config' "$tmp/cm2.json")" || true
if [[ -z "$CONFIG_JSON_STR_2" || "$CONFIG_JSON_STR_2" == "null" ]]; then
  echo "Error: .data.config not found in ConfigMap (second fetch)."
  exit 1
fi

printf '%s' "$CONFIG_JSON_STR_2" \
  | jq '."revert_config.should_revert" = false | .' \
  | jq -S '.' > "$tmp/pretty_false2.json"
jq --rawfile cfg "$tmp/pretty_false2.json" '.data.config = $cfg' "$tmp/cm2.json" > "$tmp/cm.false.updated.json"

echo "Writing updated YAML preview (false) to: ${OUTFILE%.yaml}.false.updated.yaml"
kubectl apply --dry-run=client -f "$tmp/cm.false.updated.json" -o yaml > "${OUTFILE%.yaml}.false.updated.yaml"

echo "Applying updated ConfigMap (should_revert=false)..."
kubectl apply -f "$tmp/cm.false.updated.json" "${NS_ARGS[@]}"

echo "Redeploying echonet..."
DEPLOY_SCRIPT="$(cd "$(dirname "$0")" && pwd)/deploy-echonet.sh"
if [[ -f "$DEPLOY_SCRIPT" ]]; then
  if [[ -x "$DEPLOY_SCRIPT" ]]; then
    if [[ -n "$NAMESPACE" ]]; then
      "$DEPLOY_SCRIPT" -n "$NAMESPACE"
    else
      "$DEPLOY_SCRIPT"
    fi
  else
    echo "Note: $DEPLOY_SCRIPT is not executable; attempting to run via bash..."
    if [[ -n "$NAMESPACE" ]]; then
      bash "$DEPLOY_SCRIPT" -n "$NAMESPACE"
    else
      bash "$DEPLOY_SCRIPT"
    fi
  fi
else
  echo "Warning: deploy-echonet.sh not found at $DEPLOY_SCRIPT"
fi

echo "Restarting StatefulSet '${STATEFULSET_NAME}' (phase 2)..."
kubectl scale statefulset "$STATEFULSET_NAME" --replicas=0 "${NS_ARGS[@]}"
sleep 3
kubectl scale statefulset "$STATEFULSET_NAME" --replicas=1 "${NS_ARGS[@]}"

echo "Waiting for pod/${POD_NAME} to be Ready (phase 2)..."
kubectl wait --for=condition=ready "pod/${POD_NAME}" --timeout=300s "${NS_ARGS[@]}"

 
 
 
echo "Done."

