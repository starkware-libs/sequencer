function(replacers)
  {
    db_path: '/data/committer',
    reader_config: {
      build_storage_tries_concurrently: true,
      warn_on_trivial_modifications: false,
    },
    storage_config: {
      cache_size: std.get(replacers, 'committer_config.storage_config.cache_size', 10000000),
      include_inner_stats: true,
      inner_storage_config: {
        bloom_filter_bits: 10,
        bytes_per_sync: 1048576,
        cache_size: std.get(replacers, 'committer_config.storage_config.inner_storage_config.cache_size', 8589934592),
        enable_statistics: true,
        max_background_jobs: 8,
        max_subcompactions: 8,
        max_write_buffers: 4,
        num_threads: 8,
        spawn_blocking_reads: true,
        use_mmap_reads: false,
        write_buffer_size: 134217728,
      },
    },
    verify_state_diff_hash: std.get(replacers, 'committer_config.verify_state_diff_hash', true),
  }
