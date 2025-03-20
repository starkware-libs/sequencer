#!/bin/bash

log() {
    local level="$1"
    local message="$2"
    local function_name="${FUNCNAME[1]}"  # Automatically get the caller function name
    local timestamp
    timestamp=$(date '+%Y-%m-%d %H:%M:%S')
    local newline="\n"

    # If the third argument is '-n', set newline to an empty string (no newline)
    if [[ "$3" == "-n" ]]; then
        newline=""
    fi

    # Print the log message with the appropriate color and formatting
    case "$level" in
        SUCCESS)
          printf "\033[1;32mOK\033[0m\n"
          ;;
        FAILURE)
          printf "\033[1;31mFail\033[0m\n"
          ;;
        INFO)
            printf "\033[1;34m%s [%s] [INFO]:\033[0m %s$newline" "$timestamp" "$function_name" "$message" # Blue for INFO
            ;;
        WARNING)
            printf "\033[1;33m%s [%s] [WARNING]:\033[0m %s$newline" "$timestamp" "$function_name" "$message" # Yellow for WARNING
            ;;
        ERROR)
            printf "\033[1;31m%s [%s] [ERROR]:\033[0m %s$newline" "$timestamp" "$function_name" "$message" # Red for ERROR
            ;;
        *)
            printf "\033[1;37m%s [%s] [UNKNOWN]:\033[0m %s$newline" "$timestamp" "$function_name" "$message" # Gray for unknown levels
            ;;
    esac
}

# Help function
function show_help() {
  echo "Usage: $0 [OPTIONS]"
  echo ""
  echo "Options:"
  echo "  --configs-dir CONFIGS_DIR                           Path to sequencer configs dir."
  echo "  --data-dir DATA_DIR                                 Path to sequencer fake state data dir."
  echo "  --configs-diff-dir CONFIGS-DIFF-DIR                 Path to additional config json to merge with the main config."
  echo "  --cdk8s-root-dir CDK8S_ROOT_DIR                     Path to cdk8s root dir. Usually under {sequencer repo}/deployments/sequencer"
  echo "  --namespace NAMESPACE                               K8s namespace to deploy sequencer."
  echo "  --skip-requirements-check SKIP_REQUIREMENTS_CHECK   K8s namespace to deploy sequencer."
  echo "  --help                                              Show this help message"
  echo ""
}

SKIP_REQUIREMENTS_CHECK=false
CDK8S_ROOT_DIR="./deployments/sequencer"

while [[ $# -gt 0 ]]; do
  case $1 in
    --configs-dir)
      CONFIGS_DIR="$2"
      shift 2
      ;;
    --data-dir)
      DATA_DIR="$2"
      shift 2
      ;;
    --configs-diff-dir)
      CONFIGS_DIFF_DIR="$2"
      shift 2
      ;;
    --cdk8s-root-dir)
      CDK8S_ROOT_DIR="$2"
      shift 2
      ;;
    --namespace)
      NAMESPACE="$2"
      shift 2
      ;;
    --skip-requirements-check)
      SKIP_REQUIREMENTS_CHECK=true
      shift
      ;;
    --help)
      show_help
      exit 0
      ;;
    *)
      echo "Unknown option: $1"
      show_help
      exit 1
      ;;
  esac
done

# Validate required arguments
if [[ -z "$CONFIGS_DIR" ]]; then
  log ERROR "--configs-dir is required."
  show_help
  exit 1
fi

if [[ -z "$DATA_DIR" ]]; then
  log ERROR "--data-dir is required."
  show_help
  exit 1
fi

# if [[ -z "$CDK8S_ROOT_DIR" ]]; then
#   log ERROR "--cdk8s-root-dir is required."
#   show_help
#   exit 1
# fi

if [[ -z "$NAMESPACE" ]]; then
  log ERROR "--namespace is required."
  show_help
  exit 1
fi

# Optionally, validate that the directories exist
if [[ ! -d "$CONFIGS_DIR" ]]; then
  log ERROR "--configs-dir '$CONFIGS_DIR' is not a valid directory."
  exit 1
fi

if [[ ! -d "$DATA_DIR" ]]; then
  log ERROR "--data-dir '$DATA_DIR' is not a valid directory."
  exit 1
fi

if [[ -n "$CONFIGS_DIFF_DIR" && ! -d "$CONFIGS_DIFF_DIR" ]]; then
  log ERROR "--configs-diff-dir '$CONFIGS_DIFF_DIR' is not a valid directory."
  exit 1
fi

# Validate the provided diff directory structure
if [[ -n "$CONFIGS_DIFF_DIR" ]]; then
  if ! find "$CONFIGS_DIFF_DIR" -type d -path "$CONFIGS_DIFF_DIR/node_*" -path "$CONFIGS_DIFF_DIR/node_*/executable_*" | grep -q .; then
    log ERROR "The provided diff directory '$CONFIGS_DIFF_DIR' does not have the expected structure."
    log ERROR "Expected structure: */node_*/executable_*"
    exit 1
  fi
  log INFO "Diff directory structure is valid."
fi

if [[ ! -d "$CDK8S_ROOT_DIR" ]]; then
  log ERROR "--cdk8s-root-dir '$CDK8S_ROOT_DIR' is not a valid directory."
  exit 1
fi

function check_kubectl() {
  if command -v kubectl &> /dev/null; then
    log INFO "kubectl is installed."
  else
    log ERROR "kubectl is not installed."
    log INFO "Please install kubectl and try again. https://kubernetes.io/docs/tasks/tools/install-kubectl-linux/"
    exit 1
  fi
}

function check_gcloud() {
  if command -v gcloud &> /dev/null; then
    log INFO "gcloud is installed."
  else
    log ERROR "gcloud is not installed."
    log INFO "Please install gcloud and try again. https://cloud.google.com/sdk/docs/install"
    exit 1
  fi
}

function check_gke_gcloud_auth_plugin() {
  if gcloud components list --filter="gke-gcloud-auth-plugin" --quiet 2>/dev/null | grep 'Installed' > /dev/null; then
    log INFO "gke-gcloud-auth-plugin is installed."
  else
    log ERROR "gke-gcloud-auth-plugin is not installed."
    log INFO "Please install the gke-gcloud-auth-plugin by running: sudo apt install google-cloud-cli-gke-gcloud-auth-plugin"
    exit 1
  fi
}

function check_pipenv() {
  if command -v pipenv &> /dev/null; then
    log INFO "pipenv is installed."
  else
    log ERROR "pipenv is not installed."
    log INFO "Please install pipenv and try again. https://pipenv.pypa.io/en/latest/install/"
    exit 1
  fi
}

function check_cdk8s() {
  if command -v cdk8s &> /dev/null; then
    log INFO "cdk8s is installed."
  else
    log ERROR "cdk8s is not installed."
    log INFO "Please install cdk8s and try again. https://cdk8s.io/"
    exit 1
  fi
}

function check_python3_10() {
  if command -v python3.10 &> /dev/null; then
    log INFO "Python 3.10 is installed."
  else
    log ERROR "Python 3.10 is not installed."
    log INFO "Please install Python 3.10 and try again. https://www.python.org/downloads/release/python-310/"
    exit 1
  fi
}

function check_jq() {
  if command -v jq &> /dev/null; then
    log INFO "jq is installed."
  else
    log ERROR "jq is not installed."
    log INFO "Please install jq by running: sudo apt install jq"
    exit 1
  fi
}

function requirements_check() {
  check_gcloud
  check_gke_gcloud_auth_plugin
  check_pipenv
  check_cdk8s
  check_python3_10
  check_kubectl
  check_jq
}

function find_configs() {
  local configs
  mapfile -t configs < <(find "$CONFIGS_DIR" -type f -name '*.json' -exec realpath {} \;)
  printf "%s\n" "${configs[@]}"  # Use printf to output each file in a new line
}

function find_diff_configs() {
  local diff_configs
  mapfile -t diff_configs < <(find "$CONFIGS_DIFF_DIR" -type f -name '*.json' -exec realpath {} \;)
  printf "%s\n" "${diff_configs[@]}"
}

function find_match_diff_config() {
  local config_file
  local diff_configs
  local config_file_executable_number

  config_file="$1"
  mapfile -t diff_configs < <(find_diff_configs)
  config_file_node_number=$(echo "$config_file" | sed -n 's|.*/node_\([0-9]\+\)/.*|\1|p')
  config_file_executable_number=$(echo "$config_file" | sed -n 's|.*/executable_\([0-9]\+\)/.*|\1|p')

  # Search through the list of diff_configs for the corresponding diff file
  for diff_file in "${diff_configs[@]}"; do
    diff_file_node=$(echo "$diff_file" | sed -n 's|.*/node_\([0-9]\+\)/.*|\1|p')
    diff_file_executable=$(echo "$diff_file" | sed -n 's|.*/executable_\([0-9]\+\)/.*|\1|p')
    # Check if the diff file matches the executable number
    if [[ "$config_file_node_number" == "$diff_file_node" && "$config_file_executable_number" == "$diff_file_executable" ]]; then
      echo "$diff_file"
      return
    fi
  done

  # If no match found, print a message or handle it
  echo ""
}

function merge_configs() {
  local configs
  configs=("$@")

  for config in "${configs[@]}"; do
    match_diff_config_file=$(find_match_diff_config "$config")
    if [[ -n $match_diff_config_file ]]; then
      log INFO "Patching config: $config with $match_diff_config_file"
      jq -s '.[0] * .[1]' "$config" "$match_diff_config_file" > temp.json
      mv temp.json "$config"
      log INFO "$config successfully patched."
    fi
  done
}

function generate_k8s_manifests() {
  local configs
  local base_cdk8s_cmd
  configs=("$@")
  base_cdk8s_cmd="pipenv run python main.py --namespace $NAMESPACE"

  IFS=$'\n' sorted_configs=($(sort <<<"${configs[*]}"))
  unset IFS

  for config in "${sorted_configs[@]}"; do
      base_cdk8s_cmd+=" --config-file $config"
  done

  pushd "$CDK8S_ROOT_DIR" > /dev/null || exit 1
    pipenv install
    cdk8s import
    log INFO "Executing: cdk8s synth --app \"$base_cdk8s_cmd\""
    cdk8s synth --app \""$base_cdk8s_cmd"\" > >(while IFS= read -r line; do log INFO "$line"; done) 2> >(while IFS= read -r line; do log ERROR "$line"; done)
  popd > /dev/null || exit 1
}

function deploy_sequencer() {
  pushd "$CDK8S_ROOT_DIR" > /dev/null || exit 1
    kubectl create namespace "$NAMESPACE"
    kubectl apply -R -f ./dist/ > >(while IFS= read -r line; do log INFO "$line"; done) 2> >(while IFS= read -r line; do log ERROR "$line"; done)
  popd > /dev/null || exit 1
}

function find_pod_by_label() {
  local label="$1"
  local pod

  # Find the pod using the label selector
  pod=$(kubectl get pods -l app="$label" -o custom-columns=":metadata.name" --no-headers)
  # Ensure that a pod is found
  if [ -z "$pod" ]; then
      log ERROR "No pod found for $label."
      exit 1
  fi
  echo "$pod"  # Output the pod name
}

function check_pod_status() {
  local pod="$1"
  local max_retries=10
  local retries=0
  local delay=3
  local pod_status

  log INFO "Checking pod status..." -n
  while (( retries < "$max_retries" )); do
    pod_status=$(kubectl get pod "$pod" -o jsonpath='{.status.phase}')
    if [[ "$pod_status" == "Running" ]]; then
      log SUCCESS
      return 0
    elif [[ "$pod_status" == "Pending" ]]; then
      echo -n "."
      retries=$((retries + 1))
      sleep "$delay"
    else
      log FAILURE
      log ERROR "Pod $pod fails to start. Current state: $pod_status"
      exit 1
    fi
  done

  log ERROR "Pod $pod did not reach 'Running' state after $max_retries retries."
  exit 1
}

function delete_state() {
  local pod="$1"
  check_pod_status "$pod"

  if kubectl exec "$pod" -- test -d /data/node_0; then
    log INFO "Deleting sequencer fake state data ($pod)...**" -n
    if kubectl exec "$pod" -- rm -rf /data/node_0; then
      log SUCCESS
    else
      log FAILURE
      log ERROR "Failed to delete data from $pod.**"
      exit 1
    fi
  else
    log INFO "No state data found on $pod. Skipping deletion.**"
  fi
}



function copy_data() {
  local pod="$1"
  local node_dirs

  check_pod_status "$pod"

  node_dirs=("$DATA_DIR"/node_*)
  if (( ${#node_dirs[@]} == 1 )); then
    log INFO "Copying fake state data ${DATA_DIR}/node_0 => ($pod)..." -n
    kubectl cp "${DATA_DIR}/node_0" "${pod}":/data/
  elif (( ${#node_dirs[@]} > 1 )); then
    for node_dir in "${node_dirs[@]}"; do
      node_dir_suffix="$(basename "$node_dir" | sed 's/node_//')"
      if [[ "$pod" = *node-"${node_dir_suffix}"-* ]]; then
        log INFO "Copying fake state data $node_dir => ($pod)..." -n
        kubectl cp "$node_dir" "${pod}":/data/
        break
      fi
    done
  fi

  if [ $? -ne 0 ]; then
      log FAILURE
      log ERROR "Failed to copy new data to $pod."
      exit 1
  else
    log SUCCESS
  fi
}

function restart_pod() {
  local pod="$1"
  log INFO "Restarting Sequencer ($pod)..." -n
  res=$(kubectl delete pod "$pod")
  if [ $? -ne 0 ]; then
      log FAILURE
      log ERROR "Failed to delete pod $pod."
      echo "$res"
      exit 1
  else
    log SUCCESS
  fi
}

function wait_for_pod_ready() {
  local label="$1"

  log INFO "Waiting for the new pod to become ready..." -n
  new_pod=$(kubectl get pods -l app="$label" -o custom-columns=":metadata.name" --no-headers)
  kubectl wait --for=condition=Ready pod/"${new_pod}" --timeout=60s > /dev/null
  if [ $? -ne 0 ]; then
      log FAILURE
      log ERROR "Failed to wait for pod $new_pod to be ready."
      exit 1
  else
    log SUCCESS
  fi
}

function set_namespace() {
  log INFO "Setting default namespace: $NAMESPACE"
  kubectl config set-context --current --namespace="$NAMESPACE" > /dev/null
}

function main() {
  local configs
  local diff_configs
  mapfile -t configs < <(find_configs)

  echo -e "Found ${#configs[@]} configs under $CONFIGS_DIR.\n"
  if [[ $SKIP_REQUIREMENTS_CHECK != true ]]; then
    log INFO "Starting Requirements Check..."
    requirements_check
  fi
  if [[ -n "$CONFIGS_DIFF_DIR" ]]; then
    log INFO "Starting Config Merge..."
    merge_configs "${configs[@]}"
  fi
  log INFO "Generating K8s Manifests..."
  generate_k8s_manifests "${configs[@]}"
  sleep 1
  log INFO "Deploying Sequencer..."
  deploy_sequencer
  set_namespace
  sleep 3

  for i in "${!configs[@]}"; do
    log INFO "Initializing sequencer-node-${i}..."
    log INFO "Getting pod name..."
    pod=$(find_pod_by_label  "sequencer-node-${i}")
    log INFO "Found pod: $pod"
    delete_state "$pod"
    copy_data "$pod"
    restart_pod "$pod"
    wait_for_pod_ready "sequencer-node-${i}"
  done

  echo -e "\n"
  log INFO "Operation completed successfully..."
  exit 0
}

main
