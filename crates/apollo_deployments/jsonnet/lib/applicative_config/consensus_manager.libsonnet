function(chain_params, node_params, replacers)
  local chainId = chain_params.chain_id;
  local recorderUrl = chain_params.recorder_url;
  local validatorId = node_params.validator_id;
  local revertConfig = { revert_up_to_and_including: 18446744073709551615, should_revert: false };
  local behaviorMode = 'starknet';
  {
    assume_no_malicious_validators: true,
    broadcast_buffer_size: 10000,
    cende_config: {
      max_retry_duration_secs: 3,
      max_retry_interval_ms: 1000,
      min_retry_interval_ms: 50,
      recorder_url: recorderUrl,
    },
    consensus_manager_config: {
      dynamic_config: {
        far_behind_proposal_threshold: 30,
        future_msg_limit: {
          future_height_limit: 20,
          future_height_round_limit: 5,
          future_round_limit: 20,
        },
        require_virtual_proposer_vote: std.get(replacers, 'consensus_manager_config.consensus_manager_config.dynamic_config.require_virtual_proposer_vote', false),
        stop_at_height: null,
        sync_retry_interval: 1.0,
        timeouts: {
          precommit: {
            base: 1.0,
            delta: 0.5,
            max: 5.0,
          },
          prevote: {
            base: 0.3,
            delta: 0.1,
            max: 1.0,
          },
          proposal: {
            base: std.get(replacers, 'consensus_manager_config.consensus_manager_config.dynamic_config.timeouts.proposal.base', 9.1),
            delta: 0.0,
            max: std.get(replacers, 'consensus_manager_config.consensus_manager_config.dynamic_config.timeouts.proposal.max', 9.1),
          },
        },
        validator_id: validatorId,
      },
      static_config: {
        skip_last_voted_height_check: false,
        startup_delay: 15,
        storage_config: {
          db_config: {
            chain_id: chainId,
            enforce_file_exists: false,
            growth_step: 67108864,
            max_readers: 8192,
            max_size: 1099511627776,
            min_size: 1048576,
            path_prefix: '/data/consensus',
          },
          mmap_file_config: {
            growth_step: 2147483648,
            max_object_size: 1073741824,
            max_size: 1099511627776,
          },
          scope: 'StateOnly',
        },
      },
    },
    context_config: {
      dynamic_config: {
        build_proposal_margin_millis: std.get(replacers, 'consensus_manager_config.context_config.dynamic_config.build_proposal_margin_millis', 1000),
        compare_retrospective_block_hash: std.get(replacers, 'consensus_manager_config.context_config.dynamic_config.compare_retrospective_block_hash', true),
        l1_data_gas_price_multiplier_ppt: 135,
        l1_gas_tip_wei: 1000000000,
        max_l1_data_gas_price_wei: 1000000000000,
        max_l1_gas_price_wei: 1000000000000,
        min_l1_data_gas_price_wei: 1,
        min_l1_gas_price_wei: 1000000000,
        min_l2_gas_price_per_height: std.get(replacers, 'consensus_manager_config.context_config.dynamic_config.min_l2_gas_price_per_height', ''),
        override_eth_to_fri_rate: std.get(replacers, 'consensus_manager_config.context_config.dynamic_config.override_eth_to_fri_rate', null),
        override_l1_data_gas_price_fri: std.get(replacers, 'consensus_manager_config.context_config.dynamic_config.override_l1_data_gas_price_fri', null),
        override_l1_gas_price_fri: std.get(replacers, 'consensus_manager_config.context_config.dynamic_config.override_l1_gas_price_fri', null),
        override_l2_gas_price_fri: std.get(replacers, 'consensus_manager_config.context_config.dynamic_config.override_l2_gas_price_fri', null),
        snip35_target_atto_usd_per_l2_gas: 880000000,
      },
      static_config: {
        behavior_mode: behaviorMode,
        block_timestamp_window_seconds: 1,
        build_proposal_time_ratio_for_retrospective_block_hash: 0.7,
        builder_address: '0x1176a1bd84444c89232ec27754698e5d2e7e1a7f1539f12027f28b23ec9f3d8',
        chain_id: chainId,
        l1_da_mode: true,
        proposal_buffer_size: 512,
        retrospective_block_hash_retry_interval_millis: 500,
        validate_proposal_margin_millis: 10000,
      },
    },
    network_config: {
      advertised_multiaddr: chain_params.consensus_manager_config.network_config.advertised_multiaddr,
      bootstrap_peer_multiaddr: chain_params.consensus_manager_config.network_config.bootstrap_peer_multiaddr,
      broadcasted_message_metadata_buffer_size: 100000,
      chain_id: chainId,
      discovery_config: {
        bootstrap_dial_retry_config: {
          base_delay_millis: 2,
          factor: 5,
          max_delay_seconds: 5,
          new_connection_stabilization_millis: 2000,
        },
        heartbeat_interval: 100,
      },
      idle_connection_timeout: 120,
      peer_manager_config: {
        malicious_timeout_seconds: 0,
        unstable_timeout_millis: 0,
      },
      port: std.get(replacers, 'consensus_manager_config.network_config.port', 53080),
      prune_dead_connections_ping_interval: 15,
      prune_dead_connections_ping_timeout: 20,
      reported_peer_ids_buffer_size: 100000,
      secret_key: '',
      session_timeout: 120,
    },
    proposals_topic: 'consensus_proposals',
    revert_config: revertConfig,
    staking_manager_config: {
      dynamic_config: {
        default_committee: chain_params.consensus_manager_config.staking_manager_config.dynamic_config.default_committee,
        override_committee: std.get(replacers, 'consensus_manager_config.staking_manager_config.dynamic_config.override_committee', null),
      },
      static_config: {
        max_cached_epochs: 10,
        use_only_actual_proposer_selection: true,
      },
    },
    stream_handler_config: {
      channel_buffer_capacity: 1000,
      max_message_buffer_size: 1000,
      max_peers: 100,
      max_streams: 100,
    },
    votes_topic: 'consensus_votes',
  }
