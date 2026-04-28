#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONFIG_DIR="${SCRIPT_DIR}/config"
TEMPLATE_FILE="${CONFIG_DIR}/validation_node.json.template"
OUTPUT_FILE="${CONFIG_DIR}/validation_node.json"
ENVIRONMENTS_DIR="${SCRIPT_DIR}/environments"
LAST_CHOICES_FILE="${CONFIG_DIR}/.last_choices.json"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m'

print_header() {
    echo -e "\n${BOLD}${BLUE}=== $1 ===${NC}\n"
}

print_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}⚠ $1${NC}"
}

print_error() {
    echo -e "${RED}✗ $1${NC}"
}

prompt() {
    local var_name="$1"
    local prompt_text="$2"
    local default_value="${3:-}"
    local value

    if [[ -n "${default_value}" ]]; then
        read -rp "$(echo -e "${BOLD}${prompt_text}${NC} [${default_value}]: ")" value
        value="${value:-${default_value}}"
    else
        while true; do
            read -rp "$(echo -e "${BOLD}${prompt_text}${NC}: ")" value
            if [[ -n "${value}" ]]; then
                break
            fi
            print_error "This field is required."
        done
    fi

    printf -v "${var_name}" '%s' "${value}"
}

prompt_list() {
    local var_name="$1"
    local prompt_text="$2"
    local separator="$3"
    local items=()
    local item

    echo -e "${BOLD}${prompt_text}${NC}"
    echo -e "(Enter one value per line, press Enter on an empty line when done)"
    while true; do
        read -rp "  > " item
        if [[ -z "${item}" ]]; then
            if [[ ${#items[@]} -eq 0 ]]; then
                print_error "At least one value is required."
                continue
            fi
            break
        fi
        items+=("${item}")
    done

    local joined
    joined="$(printf "${separator}%s" "${items[@]}")"
    joined="${joined#${separator}}"
    printf -v "${var_name}" '%s' "${joined}"
}

# Prompts to keep the last value or enter a new single value.
prompt_keep_or_new() {
    local var_name="$1"
    local prompt_text="$2"
    local last_value="$3"
    local default_value="${4:-}"

    echo -e "${BOLD}${prompt_text}${NC}"
    echo -e "  Last value: ${last_value}"
    read -rp "  Keep this value? [Y/n]: " keep
    if [[ "${keep}" =~ ^[Nn]$ ]]; then
        prompt "${var_name}" "${prompt_text}" "${default_value}"
    else
        printf -v "${var_name}" '%s' "${last_value}"
    fi
}

# Prompts to keep the last value or enter new values for a list field.
prompt_list_keep_or_new() {
    local var_name="$1"
    local prompt_text="$2"
    local last_value="$3"
    local separator="$4"

    echo -e "${BOLD}${prompt_text}${NC}"
    echo -e "  Last value: ${last_value}"
    read -rp "  Keep this value? [Y/n]: " keep
    if [[ "${keep}" =~ ^[Nn]$ ]]; then
        prompt_list "${var_name}" "${prompt_text}" "${separator}"
    else
        printf -v "${var_name}" '%s' "${last_value}"
    fi
}

read_env_value() {
    local env_file="$1"
    local key="$2"
    python3 -c "import json; d=json.load(open('${env_file}')); print(d['${key}'])"
}

read_last_choice() {
    local key="$1"
    python3 -c "import json; d=json.load(open('${LAST_CHOICES_FILE}')); print(d.get('${key}', ''), end='')" 2>/dev/null || true
}

save_choices() {
    ENV_NAME="${ENV_NAME}" \
    VALIDATOR_ID="${VALIDATOR_ID}" \
    L1_ENDPOINT_URLS="${L1_ENDPOINT_URLS}" \
    ETH_TO_STRK_ORACLE_URLS="${ETH_TO_STRK_ORACLE_URLS}" \
    CONSENSUS_ADVERTISED_MULTIADDR="${CONSENSUS_ADVERTISED_MULTIADDR}" \
    CONSENSUS_P2P_PORT="${CONSENSUS_P2P_PORT}" \
    LAST_CHOICES_FILE="${LAST_CHOICES_FILE}" \
    python3 - <<'PYEOF'
import json
import os

choices = {
    "environment":                    os.environ["ENV_NAME"],
    "validator_id":                   os.environ["VALIDATOR_ID"],
    "ordered_l1_endpoint_urls":       os.environ["L1_ENDPOINT_URLS"],
    "url_header_list":                os.environ["ETH_TO_STRK_ORACLE_URLS"],
    "consensus_advertised_multiaddr": os.environ["CONSENSUS_ADVERTISED_MULTIADDR"],
    "consensus_p2p_port":             os.environ["CONSENSUS_P2P_PORT"],
}
with open(os.environ["LAST_CHOICES_FILE"], "w") as f:
    json.dump(choices, f, indent=2)
PYEOF
    chmod 600 "${LAST_CHOICES_FILE}"
}

select_environment() {
    echo "  1) production  (Mainnet - SN_MAIN)"
    echo "  2) test        (Sepolia testnet - SN_SEPOLIA)"
    echo "  3) integration (Integration testnet - SN_INTEGRATION_SEPOLIA)"
    echo ""
    while true; do
        read -rp "$(echo -e "${BOLD}Select environment [1-3]: ${NC}")" env_choice
        case "${env_choice}" in
            1) ENV_FILE="${ENVIRONMENTS_DIR}/production.json";   ENV_NAME="production";   break ;;
            2) ENV_FILE="${ENVIRONMENTS_DIR}/test.json";         ENV_NAME="test";         break ;;
            3) ENV_FILE="${ENVIRONMENTS_DIR}/integration.json";  ENV_NAME="integration";  break ;;
            *) print_error "Please enter 1, 2, or 3." ;;
        esac
    done
    print_success "Selected: ${ENV_NAME}"
}

load_env_file() {
    case "${ENV_NAME}" in
        production)  ENV_FILE="${ENVIRONMENTS_DIR}/production.json" ;;
        test)        ENV_FILE="${ENVIRONMENTS_DIR}/test.json" ;;
        integration) ENV_FILE="${ENVIRONMENTS_DIR}/integration.json" ;;
    esac
    CHAIN_ID="$(read_env_value "${ENV_FILE}" chain_id)"
    ETH_FEE_TOKEN_ADDRESS="$(read_env_value "${ENV_FILE}" eth_fee_token_address)"
    STRK_FEE_TOKEN_ADDRESS="$(read_env_value "${ENV_FILE}" strk_fee_token_address)"
    STARKNET_URL="$(read_env_value "${ENV_FILE}" starknet_url)"
    STARKNET_CONTRACT_ADDRESS="$(read_env_value "${ENV_FILE}" starknet_contract_address)"
    BPO1="$(read_env_value "${ENV_FILE}" bpo1_start_block_number)"
    BPO2="$(read_env_value "${ENV_FILE}" bpo2_start_block_number)"
    FUSAKA="$(read_env_value "${ENV_FILE}" fusaka_no_bpo_start_block_number)"
    CONSENSUS_BOOTSTRAP="$(read_env_value "${ENV_FILE}" consensus_bootstrap_peer_multiaddr)"
    DEFAULT_COMMITTEE="$(read_env_value "${ENV_FILE}" default_committee)"
}

collect_user_values() {
    print_header "Required Configuration"

    prompt VALIDATOR_ID \
        "Validator ID (your validator's public key, e.g. 0x1234...abcd)"

    echo ""
    prompt_list L1_ENDPOINT_URLS \
        "L1 (Ethereum) endpoint URLs (for redundancy, enter one per line)" \
        " "

    echo ""
    prompt_list ETH_TO_STRK_ORACLE_URLS \
        "ETH-to-STRK oracle URL+headers (enter one per line)
  Format: https://api.example.com/endpoint,header_key^header_value" \
        "|"

    echo ""
    prompt_list CONSENSUS_ADVERTISED_MULTIADDR \
        "Consensus advertised multiaddr (externally reachable address(es) for P2P)
  Format: /ip4/<your-public-ip>/tcp/<port> or /dns/<hostname>/tcp/<port>" \
        ","

    echo ""
    print_header "Optional Configuration"
    prompt CONSENSUS_P2P_PORT \
        "Consensus P2P port (optional, press Enter for default)" \
        "53080"
}

collect_user_values_per_setting() {
    local last_env="$1"
    print_header "Review Each Setting"

    echo -e "${BOLD}Environment:${NC}"
    echo -e "  Last value: ${last_env}"
    read -rp "  Keep this value? [Y/n]: " keep_env
    if [[ "${keep_env}" =~ ^[Nn]$ ]]; then
        select_environment
    else
        ENV_NAME="${last_env}"
    fi

    echo ""
    prompt_keep_or_new VALIDATOR_ID \
        "Validator ID (your validator's public key, e.g. 0x1234...abcd)" \
        "$(read_last_choice validator_id)"

    echo ""
    prompt_list_keep_or_new L1_ENDPOINT_URLS \
        "L1 (Ethereum) endpoint URLs (for redundancy, enter one per line)" \
        "$(read_last_choice ordered_l1_endpoint_urls)" \
        " "

    echo ""
    prompt_list_keep_or_new ETH_TO_STRK_ORACLE_URLS \
        "ETH-to-STRK oracle URL+headers (enter one per line)
  Format: https://api.example.com/endpoint,header_key^header_value" \
        "$(read_last_choice url_header_list)" \
        "|"

    echo ""
    prompt_list_keep_or_new CONSENSUS_ADVERTISED_MULTIADDR \
        "Consensus advertised multiaddr (externally reachable address(es) for P2P)
  Format: /ip4/<your-public-ip>/tcp/<port> or /dns/<hostname>/tcp/<port>" \
        "$(read_last_choice consensus_advertised_multiaddr)" \
        ","

    echo ""
    prompt_keep_or_new CONSENSUS_P2P_PORT \
        "Consensus P2P port (optional, press Enter to use the default)" \
        "$(read_last_choice consensus_p2p_port)" \
        "53080"
}

generate_config() {
    print_header "Generating Configuration"

    escape_sed() {
        printf '%s' "$1" | sed 's/[\/&]/\\&/g'
    }

    sed \
        -e "s/{{validator_id}}/$(escape_sed "${VALIDATOR_ID}")/g" \
        -e "s/{{ordered_l1_endpoint_urls}}/$(escape_sed "${L1_ENDPOINT_URLS}")/g" \
        -e "s/{{url_header_list}}/$(escape_sed "${ETH_TO_STRK_ORACLE_URLS}")/g" \
        -e "s/{{consensus_advertised_multiaddr}}/$(escape_sed "${CONSENSUS_ADVERTISED_MULTIADDR}")/g" \
        -e "s/{{consensus_p2p_port}}/${CONSENSUS_P2P_PORT}/g" \
        -e "s/{{chain_id}}/$(escape_sed "${CHAIN_ID}")/g" \
        -e "s/{{eth_fee_token_address}}/$(escape_sed "${ETH_FEE_TOKEN_ADDRESS}")/g" \
        -e "s/{{strk_fee_token_address}}/$(escape_sed "${STRK_FEE_TOKEN_ADDRESS}")/g" \
        -e "s/{{starknet_url}}/$(escape_sed "${STARKNET_URL}")/g" \
        -e "s/{{starknet_contract_address}}/$(escape_sed "${STARKNET_CONTRACT_ADDRESS}")/g" \
        -e "s/{{bpo1_start_block_number}}/${BPO1}/g" \
        -e "s/{{bpo2_start_block_number}}/${BPO2}/g" \
        -e "s/{{fusaka_no_bpo_start_block_number}}/${FUSAKA}/g" \
        -e "s/{{consensus_bootstrap_peer_multiaddr}}/$(escape_sed "${CONSENSUS_BOOTSTRAP}")/g" \
        -e "s/{{default_committee}}/$(escape_sed "${DEFAULT_COMMITTEE}")/g" \
        "${TEMPLATE_FILE}" > "${OUTPUT_FILE}"

    chmod 640 "${OUTPUT_FILE}"
    print_success "Configuration written to: ${OUTPUT_FILE}"
}

# ── Main flow ──────────────────────────────────────────────────────────────────

jump_to_docker=false

# Step 1: Check if config already exists
if [[ -f "${OUTPUT_FILE}" ]]; then
    print_warning "A settings file already exists at: ${OUTPUT_FILE}"
    echo ""
    read -rp "$(echo -e "${BOLD}Do you want to create a new one (overwrites existing)? [y/N]: ${NC}")" overwrite
    if [[ ! "${overwrite}" =~ ^[Yy]$ ]]; then
        echo ""
        echo "Keeping existing configuration."
        jump_to_docker=true
    fi
fi

if [[ "${jump_to_docker}" == false ]]; then
    if [[ -f "${LAST_CHOICES_FILE}" ]]; then
        # Step 2a: Show last choices and ask what to do
        LAST_ENV="$(read_last_choice environment)"
        print_header "Last Used Configuration"
        echo -e "  ${BOLD}Environment:${NC}              $(read_last_choice environment)"
        echo -e "  ${BOLD}Validator ID:${NC}             $(read_last_choice validator_id)"
        echo -e "  ${BOLD}L1 endpoint URLs:${NC}         $(read_last_choice ordered_l1_endpoint_urls)"
        echo -e "  ${BOLD}Oracle URLs:${NC}              $(read_last_choice url_header_list)"
        echo -e "  ${BOLD}Advertised multiaddr:${NC}     $(read_last_choice consensus_advertised_multiaddr)"
        echo -e "  ${BOLD}Consensus P2P port:${NC}       $(read_last_choice consensus_p2p_port)"
        echo ""
        read -rp "$(echo -e "${BOLD}Use all these values again? [Y/n]: ${NC}")" use_all
        if [[ ! "${use_all}" =~ ^[Nn]$ ]]; then
            ENV_NAME="${LAST_ENV}"
            VALIDATOR_ID="$(read_last_choice validator_id)"
            L1_ENDPOINT_URLS="$(read_last_choice ordered_l1_endpoint_urls)"
            ETH_TO_STRK_ORACLE_URLS="$(read_last_choice url_header_list)"
            CONSENSUS_ADVERTISED_MULTIADDR="$(read_last_choice consensus_advertised_multiaddr)"
            CONSENSUS_P2P_PORT="$(read_last_choice consensus_p2p_port)"
        else
            collect_user_values_per_setting "${LAST_ENV}"
        fi
    else
        # Step 2b: Fresh setup — select environment then collect values
        print_header "Select Environment"
        select_environment

        collect_user_values
    fi

    if ! [[ "${CONSENSUS_P2P_PORT}" =~ ^[0-9]+$ ]] || \
       (( CONSENSUS_P2P_PORT < 1 || CONSENSUS_P2P_PORT > 65535 )); then
        print_error "CONSENSUS_P2P_PORT must be an integer in [1, 65535], got '${CONSENSUS_P2P_PORT}'"
        exit 1
    fi

    load_env_file
    generate_config
    save_choices
fi

# Step 3: Ask about running docker
echo ""
print_header "Start Node"
read -rp "$(echo -e "${BOLD}Do you want to start the node with docker compose now? [y/N]: ${NC}")" start_docker

if [[ "${start_docker}" =~ ^[Yy]$ ]]; then
    echo ""
    echo "Starting docker compose..."
    cd "${SCRIPT_DIR}"
    docker compose up -d
    echo ""
    print_success "Node started. Monitor with:"
    echo "  docker compose logs -f"
    echo "  curl http://localhost:8082/monitoring/alive   # validation node"
    echo "  curl http://localhost:8083/monitoring/alive   # signature manager"
else
    echo ""
    echo "To start the node later, run from ${SCRIPT_DIR}:"
    echo "  docker compose up -d"
fi
