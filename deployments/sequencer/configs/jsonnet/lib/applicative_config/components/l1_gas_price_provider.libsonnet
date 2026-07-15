function()
  {
    eth_to_strk_oracle_config: {
      lag_interval_seconds: 900,
      max_cache_size: 100,
      query_timeout_sec: 10,
      url_header_list: 'https://api.example.com/api',
    },
    lag_margin_seconds: 600.0,
    max_time_gap_seconds: 900,
    number_of_blocks_for_mean: 300,
    storage_limit: 3000,
    strk_to_usd_oracle_config: {
      lag_interval_seconds: 900,
      max_cache_size: 100,
      query_timeout_sec: 10,
      url_header_list: 'https://api.example.com/api',
    },
    eth_to_strk_oracle_source: 'Http',
    strk_to_usd_oracle_source: 'Http',
    rate_bounds_config: {
      eth_usd: { minimum_micro_units: 20000000, maximum_micro_units: 50000000000 },
      strk_usd: { minimum_micro_units: 100, maximum_micro_units: 10000000 },
      eth_strk: { minimum_micro_units: 10000000000, maximum_micro_units: 1000000000000 },
    },
    chainlink_oracle_config: {
      eth_usd_feed_address: '0x06b2ef9b416ad0f996b2a8ac0dd771b1788196f51c96f5b000df2e47ac756d26',
      strk_usd_feed_address: '0x076a0254cdadb59b86da3b5960bf8d73779cac88edc5ae587cab3cedf03226ec',
      freshness: { max_staleness_seconds: 90000, max_future_updated_at_seconds: 300 },
      sampling_interval_seconds: 900,
      failure_retry_interval_seconds: 60,
    },
  }
