// Per-service SequencerNodeConfig for the echonet `hybrid` overlay.
// Evaluate: jsonnet -J <repo>/deployments/sequencer/configs/jsonnet node.jsonnet
local build = import 'lib/build.libsonnet';

// Recursively merges `overrides` into `base`; unlike std.mergePatch, a `null` override sets the
// field to null (a disabled optional) instead of deleting it.
local deepMerge(base, overrides) =
  if std.isObject(base) && std.isObject(overrides) then
    base + {
      [key]: if std.objectHas(base, key) then deepMerge(base[key], overrides[key]) else overrides[key]
      for key in std.objectFields(overrides)
    }
  else overrides;

local built = build.build({
  chain_params: import './chain_params.jsonnet',
  node_params: { validator_id: '0x64', node_index: 0 },
});

// Echonet values the chain_params do not expose, by config section; each service takes the sections
// it hosts. The feeder http_headers come from the node secret file.
local echonetOverrides = {
  components: {
    // Echonet's fake L1 serves no baseFeePerGas history.
    l1_gas_price_scraper: { execution_mode: 'Disabled' },
  },
  l1_gas_price_scraper_config: null,
  base_layer_config: {
    ordered_l1_endpoint_urls: 'http://echonet:80/l1',
  },
  batcher_config: {
    dynamic_config: {
      n_concurrent_txs: 500,
    },
    static_config: {
      // Block boundaries come from mainnet, so the bouncer must never close a block early.
      block_builder_config: {
        bouncer_config: {
          block_max_capacity: {
            proving_gas: 6000000000,
            receipt_l2_gas: 580000000000,
            sierra_gas: 6000000000,
            state_diff_size: 5000,
          },
          builtin_instance_limits: {
            add_mod: 1000000000000000000,
            bitwise: 1000000000000000000,
            blake: 1000000000000000000,
            ecdsa: 1000000000000000000,
            ecop: 1000000000000000000,
            keccak: 1000000000000000000,
            mul_mod: 1000000000000000000,
            pedersen: 1000000000000000000,
            poseidon: 1000000000000000000,
            range_check: 1000000000000000000,
            range_check96: 1000000000000000000,
          },
        },
      },
      propose_l1_txs_every: 1,
    },
  },
  class_manager_config: {
    static_config: {
      class_manager_config: {
        max_compiled_contract_class_object_size: 14089446,
      },
    },
  },
  config_manager_config: {
    config_update_interval_secs: 60.0,
  },
  consensus_manager_config: {
    consensus_manager_config: {
      static_config: {
        // The gateway reaches a restarted core through its Service only seconds after core is
        // ready; wait so the first replayed blocks' txs are in the mempool before proposing.
        startup_delay: 60,
      },
    },
    cende_config: {
      max_retry_duration_secs: 60,
      max_retry_interval_ms: 30000,
    },
    context_config: {
      // Prices are replayed from mainnet, so none of the sequencer's own clamps may bite.
      dynamic_config: {
        l1_data_gas_price_multiplier_ppt: 1000,
        l1_gas_tip_wei: 0,
        // jsonnet numbers are doubles: this renders as 1000000000000000013287555072.
        max_l1_data_gas_price_wei: 1e27,
        max_l1_gas_price_wei: 1e27,
        min_l1_data_gas_price_wei: 0,
        min_l1_gas_price_wei: 0,
      },
      static_config: {
        behavior_mode: 'echonet',
      },
    },
    network_config: {
      secret_key: '0x0101010101010101010101010101010101010101010101010101010101010101',
    },
  },
  gateway_config: {
    static_config: {
      behavior_mode: 'echonet',
      stateful_tx_validator_config: {
        max_allowed_nonce_gap: 500,
      },
      stateless_tx_validator_config: {
        // Replayed mainnet private txs carry V1 proof facts; main defaults V1 to rejected.
        allow_proof_version_v1: true,
        max_contract_class_object_size: 8089446,
        max_sierra_version: { patch: 1 },
      },
    },
  },
  l1_events_provider_config: {
    l1_handler_proposal_cooldown_seconds: 0,
  },
  l1_events_scraper_config: {
    // The fake L1 jumps straight to the next handler's block; a capped fetch needs one more poll
    // per 1000 blocks, which can push the handler past its block's proposal.
    max_blocks_per_fetch: 1000000,
    polling_interval_seconds: 1,
    startup_rewind_time_seconds: 3600,
  },
  l1_gas_price_provider_config: {
    eth_to_strk_oracle_config: {
      url_header_list: 'http://echonet:80/echonet/eth_to_strk_oracle',
    },
    eth_to_strk_oracle_source: 'Chainlink',
    lag_margin_seconds: 0,
    max_time_gap_seconds: 9999999999,
    number_of_blocks_for_mean: 1,
    strk_to_usd_oracle_config: {
      url_header_list: 'http://dummy-strk2usd-oracle-service.dummy-strk2usd-oracle.svc.cluster.local/strk_to_usd_oracle?timestamp=:9000',
    },
    strk_to_usd_oracle_source: 'Chainlink',
  },
  mempool_config: {
    dynamic_config: {
      transaction_ttl: 3000,
    },
    static_config: {
      behavior_mode: 'echonet',
    },
  },
  mempool_p2p_config: {
    network_config: {
      secret_key: '0x0101010101010101010101010101010101010101010101010101010101010101',
    },
  },
  sierra_compiler_config: {
    max_memory_usage: 21474836480,
  },
  state_sync_config: {
    static_config: {
      network_config: {
        secret_key: '0x0101010101010101010101010101010101010101010101010101010101010101',
      },
    },
  },
};

// Replayed mainnet blocks can carry committer read paths and compiled classes past the default
// 8 MiB body limit between services.
local bodyLimitBytes = 268435456;
local raiseBodyLimits(components) = {
  [name]: components[name] + {
    [field]: components[name][field] + {
      [limit]: bodyLimitBytes
      for limit in ['max_response_body_bytes', 'max_request_body_bytes']
      if std.objectHas(components[name][field], limit)
    }
    for field in ['remote_client_config', 'remote_server_config']
    if std.isObject(std.get(components[name], field))
  }
  for name in std.objectFields(components)
  if std.isObject(components[name])
};

{
  [service]: (
    local merged = deepMerge(built[service], {
      [section]: echonetOverrides[section]
      for section in std.objectFields(echonetOverrides)
      if std.objectHas(built[service], section)
    });
    merged + { components: raiseBodyLimits(merged.components) }
  )
  for service in std.objectFields(built)
}
