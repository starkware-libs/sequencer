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
  }
