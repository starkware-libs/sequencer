#!/usr/bin/env bash
set -euo pipefail

# ---- Configurable defaults ----
NAMESPACE=""
CONFIGMAP_NAME="sequencer-node-config"
STATEFULSET_NAME="sequencer-node-statefulset"
OUTFILE="sequencer-node-config.yaml"
DRYRUN="false"
WAIT_TIMEOUT_SECS=900   # 15 minutes for the log wait when should_revert=true
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

# Read current flag and compute toggled value
CURRENT_VAL="$(printf '%s' "$CONFIG_JSON_STR" | jq -r '."revert_config.should_revert"')" || true
if [[ "$CURRENT_VAL" != "true" && "$CURRENT_VAL" != "false" ]]; then
  echo "Error: Key \"revert_config.should_revert\" not found or not boolean in data.config."
  exit 1
fi
NEW_VAL="true"; [[ "$CURRENT_VAL" == "true" ]] && NEW_VAL="false"
echo "Toggling revert_config.should_revert: ${CURRENT_VAL} -> ${NEW_VAL}"

# Build PRETTY-PRINTED config JSON to a temp file (this is the multi-line content we store back)
printf '%s' "$CONFIG_JSON_STR" \
  | jq --argjson v "$NEW_VAL" '."revert_config.should_revert" = $v | .' \
  | jq -S '.' > "$tmp/pretty_config.json"

# Inject that pretty JSON (as a raw string) back into the ConfigMap JSON safely (no shell quoting games)
jq --rawfile cfg "$tmp/pretty_config.json" '.data.config = $cfg' "$tmp/cm.json" > "$tmp/cm.updated.json"

# Detect if anything changed
if diff -q <(jq -r '.data.config' "$tmp/cm.json") <(jq -r '.data.config' "$tmp/cm.updated.json") >/dev/null; then
  echo "No change detected (config remained the same)."
  echo "Backup left at: $OUTFILE"
  exit 0
fi

# Emit a human-friendly updated YAML preview
echo "Writing updated YAML preview to: ${OUTFILE%.yaml}.updated.yaml"
kubectl apply --dry-run=client -f "$tmp/cm.updated.json" -o yaml > "${OUTFILE%.yaml}.updated.yaml"

if [[ "$DRYRUN" == "true" ]]; then
  echo "Dry-run only. Not applying or restarting."
  exit 0
fi

# Apply change
echo "Applying updated ConfigMap..."
kubectl apply -f "$tmp/cm.updated.json" "${NS_ARGS[@]}"

# Restart statefulset (scale 0 -> 1) and wait for pod readiness
echo "Restarting StatefulSet '${STATEFULSET_NAME}'..."
kubectl scale statefulset "$STATEFULSET_NAME" --replicas=0 "${NS_ARGS[@]}"
# Small grace period for termination
sleep 3
kubectl scale statefulset "$STATEFULSET_NAME" --replicas=1 "${NS_ARGS[@]}"

POD_NAME="${STATEFULSET_NAME}-${POD_ORDINAL}"
echo "Waiting for pod/${POD_NAME} to be Ready..."
kubectl wait --for=condition=ready "pod/${POD_NAME}" --timeout=300s "${NS_ARGS[@]}"

# Branch behavior based on the new value
if [[ "$NEW_VAL" == "true" ]]; then
  # Matches JSON or plain text, with or without trailing period.
  LOG_REGEX='("message":"Starting eternal pending\.?"|Starting eternal pending\.?)'
  echo "should_revert=true → streaming logs for: Starting eternal pending (timeout: ${WAIT_TIMEOUT_SECS}s)"

  CONTAINER_ARGS=()
  if [[ -n "${CONTAINER:-}" ]]; then CONTAINER_ARGS=(-c "$CONTAINER"); fi

  if command -v timeout >/dev/null 2>&1; then
    # Use process substitution so only grep's exit code is considered.
    if timeout "${WAIT_TIMEOUT_SECS}s" \
         grep -m1 -E "$LOG_REGEX" < <(kubectl logs "pod/${POD_NAME}" "${NS_ARGS[@]}" "${CONTAINER_ARGS[@]}" --since=30m -f)
    then
      echo "✔ Detected: Starting eternal pending"
    else
      echo "✖ Timeout after ${WAIT_TIMEOUT_SECS}s waiting for 'Starting eternal pending'."
      exit 1
    fi
  else
    # Fallback without timeout(1): run grep in background and poll.
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
else
  echo "should_revert=false → starting port-forward (Ctrl+C to stop):"
  echo "kubectl ${NAMESPACE:+-n $NAMESPACE} port-forward pod/${POD_NAME} 8080:8080"
  exec kubectl "${NS_ARGS[@]}" port-forward "pod/${POD_NAME}" 8080:8080
fi



echo "Done."

