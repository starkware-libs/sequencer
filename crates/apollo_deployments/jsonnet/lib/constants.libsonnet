// Shared applicative config defaults, importable by other jsonnet files.
{
  DEFAULT_VALIDATION_ONLY: false,
  DEFAULT_ETH_FEE_TOKEN_ADDRESS: '0x049d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7',
  DEFAULT_STRK_FEE_TOKEN_ADDRESS: '0x04718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d',

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
