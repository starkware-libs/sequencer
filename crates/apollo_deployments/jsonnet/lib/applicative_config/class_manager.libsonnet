function(chain_params, replacers)
  local chainId = chain_params.chain_id;
  {
    dynamic_config: {
      storage_reader_server_dynamic_config: {
        enable: false,
      },
    },
    static_config: {
      class_manager_config: {
        cached_class_storage_config: {
          class_cache_size: 128,
          deprecated_class_cache_size: 128,
        },
        max_compiled_contract_class_object_size: replacers['class_manager_config.static_config.class_manager_config.max_compiled_contract_class_object_size'],
      },
      class_storage_config: {
        class_hash_storage_config: {
          db_config: {
            chain_id: chainId,
            enforce_file_exists: false,
            growth_step: 67108864,
            max_readers: 8192,
            max_size: 1099511627776,
            min_size: 1048576,
            path_prefix: '/data/class_manager/class_hash_storage',
          },
          mmap_file_config: {
            growth_step: 2147483648,
            max_object_size: 1073741824,
            max_size: 1099511627776,
          },
          scope: 'StateOnly',
        },
        persistent_root: '/data/class_manager/classes',
        storage_reader_server_static_config: {
          ip: '0.0.0.0',
          port: 8091,
        },
      },
    },
  }
