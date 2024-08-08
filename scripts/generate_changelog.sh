#!/bin/bash

set -e

# Usage:
# scripts/generate_changelog.sh <FROM_TAG> <TO_TAG> <PROJECT_NAME>
# Valid project names are [blockifier, mempool, papyrus, committer, starknet_api].

# Install git-cliff if missing.
GIT_CLIFF_VERSION="2.4.0"
cargo install --list | grep -q "git-cliff v${GIT_CLIFF_VERSION}" || cargo install git-cliff@${GIT_CLIFF_VERSION}

case $3 in
  blockifier)
    crates=("blockifier" "native_blockifier")
    ;;
  mempool)
    crates=("gateway" "mempool" "mempool_infra" "mempool_node" "mempool_test_utils" "mempool_types")
    ;;
  papyrus)
    crates=("papyrus_base_layer" "papyrus_common" "papyrus_config" "papyrus_execution" "papyrus_load_test"
            "papyrus_monitoring_gateway" "papyrus_network" "papyrus_node" "papyrus_p2p_sync"
            "papyrus_proc_macros" "papyrus_protobuf" "papyrus_rpc" "papyrus_storage" "papyrus_sync"
            "papyrus_test_utils" "sequencing") # is sequencing should be here?
    ;;
  committer)
    crates=("committer" "committer_cli" "starknet_committer")
    ;;
  starknet_api)
    crates=("starknet_api")
    ;;
  *)
    echo "Invalid project name was given. Must be one of [blockifier, mempool, papyrus, committer, starknet_api]."
    exit 1
    ;;
esac
# TODO: To which projects starknet_sierra_compile, task_executor, tests-integration belong?

command=""
for crate in "${crates[@]}"; do
  command+="--include-path crates/${crate}/ "
done
# Combine dev tags into the next RC / stable tag.
git-cliff $1..$2 ${command} --ignore-tags ".*-dev.[0-9]+"
