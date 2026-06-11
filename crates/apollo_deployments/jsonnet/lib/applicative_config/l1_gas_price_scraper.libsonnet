function(chain_params)
  local chainId = chain_params.chain_id;
  {
    chain_id: chainId,
    finality: 10,
    number_of_blocks_for_mean: 300,
    polling_interval: 120.0,
    starting_block: null,
    startup_num_blocks_multiplier: 2,
  }
