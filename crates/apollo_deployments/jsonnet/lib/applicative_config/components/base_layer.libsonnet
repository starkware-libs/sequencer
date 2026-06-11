function(chain_params)
  {
    bpo1_start_block_number: chain_params.base_layer.bpo1_start_block_number,
    bpo2_start_block_number: chain_params.base_layer.bpo2_start_block_number,
    fusaka_no_bpo_start_block_number: chain_params.base_layer.fusaka_no_bpo_start_block_number,
    ordered_l1_endpoint_urls: 'https://mainnet.infura.io/v3/YOUR_INFURA_API_KEY',
    retry_primary_interval_seconds: 60,
    starknet_contract_address: chain_params.starknet_contract_address,
    timeout_millis: 1000,
  }
