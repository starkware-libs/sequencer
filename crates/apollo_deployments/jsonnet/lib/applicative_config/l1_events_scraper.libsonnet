function(chain_params)
  local chainId = chain_params.chain_id;
  {
    chain_id: chainId,
    finality: 10,
    l1_block_time_seconds: 12,
    polling_interval_seconds: 30.0,
    set_provider_historic_height_to_l2_genesis: false,
    startup_rewind_time_seconds: 21600.0,
  }
