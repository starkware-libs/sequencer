#!/bin/bash

# Default values for optional arguments
name=""
config=""
namespace=""

log() {
    local level="$1"
    local message="$2"
    local function_name="${FUNCNAME[1]}"  # Automatically get the caller function name
    local timestamp
    timestamp=$(date '+%Y-%m-%d %H:%M:%S')
    local echo_opts="-e"  # Default to -e for interpreted escape sequences

    # If the third argument is '-n', set echo_opts to '-en' to prevent newline
    if [[ "$3" == "-n" ]]; then
        echo_opts="-en"
    fi

    # Print the log message with the appropriate color and formatting
    case "$level" in
        INFO)
            echo $echo_opts "\033[1;34m$timestamp [$function_name] [INFO]: $message\033[0m" # Blue for INFO
            ;;
        WARNING)
            echo $echo_opts "\033[1;33m$timestamp [$function_name] [WARNING]: $message\033[0m" # Yellow for WARNING
            ;;
        ERROR)
            echo $echo_opts "\033[1;31m$timestamp [$function_name] [ERROR]: $message\033[0m" # Red for ERROR
            ;;
        *)
            echo $echo_opts "\033[1;37m$timestamp [$function_name] [UNKNOWN]: $message\033[0m" # Gray for unknown levels
            ;;
    esac
}


# Function to print usage
usage() {
    echo "Usage: $0 --namespace <namespace> [--name <name>] [--config <config>]"
    echo
    echo "Arguments:"
    echo "  --namespace    (mandatory) Specify the Kubernetes namespace for deployment."
    echo "  --name         (optional)  Specify the application name. Default: 'sequencer-node'."
    echo "  --config       (optional)  Provide the path to the configuration file. Default: 'config/sequencer/presets/config.json'."
    echo
    echo "Example:"
    echo "  $0 --namespace my-namespace"
    echo "  $0 --namespace my-namespace --name my-app"
    echo "  $0 --namespace my-namespace --config /path/to/config.json"
}

# Parse arguments
while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --name)
      if [[ -z "$2" || "$2" == --* ]]; then
        log ERROR "Argument for --name is missing."
        usage
        exit 1
      fi
      name="$2"
      shift 2
      ;;
    --namespace)
      if [[ -z "$2" || "$2" == --* ]]; then
        log ERROR "Argument for --namespace is missing."
        usage
        exit 1
      fi
      namespace="$2"
      shift 2
      ;;
    --config)
      if [[ -z "$2" || "$2" == --* ]]; then
        log ERROR "Argument for --config is missing."
        usage
        exit 1
      fi
      config="$2"
      shift 2
      ;;
    *)
      log ERROR "Unknown option: $1"
      usage
      exit 1
      ;;
  esac
done

# Validate required arguments
if [[ -z "$namespace" ]]; then
  log ERROR "Missing required argument: --namespace."
  usage
  exit 1
fi

# Export variables only if the argument was provided
if [[ -n "$name" ]]; then
  export NAME="$name"
fi

if [[ -n "$namespace" ]]; then
  export NAMESPACE="$namespace"
fi

if [[ -n "$config" ]]; then
  export CONFIG="$config"
fi

# Function to deploy the namespace
deploy_namespace() {
  local output

  if [[ -n "$namespace_file" ]]; then
    log INFO "Creating namespace: $namespace..."
    output=$(kubectl apply -f "$namespace_file" 2>&1)
    if [[ $? -eq 0 ]]
    then
      log INFO "$output"
    else
      log ERROR "$output"
    fi

    log INFO "Waiting for namespace to become active..."
    output=$(kubectl wait --for=jsonpath='{.status.phase}'=Active --timeout=15s namespace/"$namespace" 2>&1)
    if [[ $? -eq 0 ]]
    then
      log INFO "$output"
      log INFO "Namespace $namespace is ready."
    else
      log ERROR "$output"
      log ERROR "Namespace $namespace is not ready within the timeout period."
      exit 1
    fi
  else
    log INFO "No namespace manifest found for $namespace."
  fi
}

# Function to deploy Kubernetes manifests
deploy_to_k8s() {
  local output

  log INFO "Deploying $name..."
  for manifest in $(find "./dist/$name" -type f -name "*.yaml" | grep -v "$namespace_file"); do
      log INFO "Deploying $manifest..."
      output=$(kubectl apply -f "$manifest" 2>&1)
      if [[ $? -eq 0 ]]; then
        log INFO "$output"
      else
        log ERROR "$output"
      fi
  done
}

cdk8s_synth() {
  local output

  # Synthesize manifests using cdk8s
  log INFO "Synthesizing Kubernetes manifests..."
  output=$(cdk8s synth 2>&1)
  if [[ $? -eq 0 ]]
  then
    echo "$output"
    echo "------------------------------------------------------------------------------------------------"
    echo ""
    log INFO "Kubernetes manifests are ready for deployment."
  else
    echo "------------------------------------------------------------------------------------------------"
    log ERROR "$output"
    echo "------------------------------------------------------------------------------------------------"
    exit 1
  fi
}

cdk8s_synth

# Confirm current Kubernetes context
current_context=$(kubectl config current-context)
log INFO "Your current Kubernetes context is: $current_context"
log INFO "Continue to deploy to Kubernetes? (yes/no): " "-n"
read response

# Deploy based on user confirmation
case "$response" in
  [yY][eE][sS]|[yY])
    log INFO "Deploying to Kubernetes..."
    namespace_file=$(find ./dist -type f -name "*Namespace.${namespace}*")
    deploy_namespace
    deploy_to_k8s
    ;;
  [nN][oO]|[nN])
    log INFO "Exiting without deployment."
    exit 0
    ;;
  *)
    log ERROR "Invalid response. Please answer 'yes' or 'no'."
    exit 1
    ;;
esac
