function(chain_params, replacers)
  local recorderUrl = chain_params.recorder_url;
  local validateResourceBounds = true;
  local behaviorMode = 'starknet';
  {
    dynamic_config: {
      transaction_ttl: replacers['mempool_config.dynamic_config.transaction_ttl'],
    },
    static_config: {
      behavior_mode: behaviorMode,
      capacity_in_bytes: 1073741824,
      committed_nonce_retention_block_count: 100,
      declare_delay: 20,
      enable_fee_escalation: true,
      fee_escalation_percentage: 10,
      recorder_url: recorderUrl,
      validate_resource_bounds: validateResourceBounds,
    },
  }
