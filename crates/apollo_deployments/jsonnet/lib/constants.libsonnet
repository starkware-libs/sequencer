{
  // Ethereum base-layer start-block numbers, keyed by the L1 network a Starknet env settles on.
  // Sepolia-settling envs (sepolia-integration, sepolia-alpha) share the testnet numbers; mainnet
  // settles on Ethereum mainnet. Consumed as a whole `base_layer_config` object by env chain_params.
  ETH_TESTNET_BASE_LAYER: {
    bpo1_start_block_number: 9456501,
    bpo2_start_block_number: 9504747,
    fusaka_no_bpo_start_block_number: 9408577,
  },
  ETH_MAINNET_BASE_LAYER: {
    bpo1_start_block_number: 23973546,
    bpo2_start_block_number: 24168146,
    fusaka_no_bpo_start_block_number: 23934586,
  },
}
