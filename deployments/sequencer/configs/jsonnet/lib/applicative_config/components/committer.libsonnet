function(chain_params)
  {
    db_path: '/data/committer',
    reader_config: {
      build_storage_tries_concurrently: true,
      warn_on_trivial_modifications: false,
    },
    storage_config: {
      cache_size: chain_params.committer_cache_size,
      // DEFAULT_SHORT_LIVED_CACHE_SIZE: the commitment infos keys of the 11 latest blocks.
      short_lived_cache_size: 22,
      include_inner_stats: true,
      inner_storage_config: {
        bloom_filter_bits: 10,
        bytes_per_sync: 1048576,
        max_read_tasks: 32,
        cache_size: chain_params.committer_inner_storage_cache_size,
        enable_statistics: true,
        extra_options: '',
        max_background_jobs: 8,
        max_subcompactions: 8,
        max_write_buffers: 4,
        num_threads: 8,
        spawn_blocking_reads: true,
        ttl_seconds: 0,
        use_mmap_reads: false,
        write_buffer_size: 134217728,
      },
    },
    commit_duration_warn_threshold_millis: 3000,
    commitment_infos_pruning_config: {
      retention_blocks: 1000,
      range_deletion_interval_blocks: 100,
    },
    verify_state_diff_hash: true,
  }
