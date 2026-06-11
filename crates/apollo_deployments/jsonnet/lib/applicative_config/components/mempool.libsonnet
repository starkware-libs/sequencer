local defaults = import 'lib/defaults.libsonnet';
function(chain_params, replacers)
  {
    dynamic_config: {
      transaction_ttl: replacers['mempool_config.dynamic_config.transaction_ttl'],
    },
    static_config: {
      behavior_mode: defaults.BEHAVIOR_MODE,
      capacity_in_bytes: 1073741824,
      committed_nonce_retention_block_count: 100,
      declare_delay: 20,
      enable_fee_escalation: true,
      fee_escalation_percentage: 10,
      recorder_url: chain_params.recorder_url,
      validate_resource_bounds: defaults.VALIDATE_RESOURCE_BOUNDS,
    },
  }
