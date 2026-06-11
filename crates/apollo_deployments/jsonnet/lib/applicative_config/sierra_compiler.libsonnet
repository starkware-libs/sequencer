function(replacers)
  local maxCpuTime = 600;
  {
    audited_libfuncs_only: std.get(replacers, 'sierra_compiler_config.audited_libfuncs_only', true),
    max_bytecode_size: std.get(replacers, 'sierra_compiler_config.max_bytecode_size', 81920),
    max_cpu_time: maxCpuTime,
    max_memory_usage: 5368709120,
  }
