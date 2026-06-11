function(replacers)
  local maxCpuTime = 600;
  {
    audited_libfuncs_only: replacers['sierra_compiler_config.audited_libfuncs_only'],
    max_bytecode_size: replacers['sierra_compiler_config.max_bytecode_size'],
    max_cpu_time: maxCpuTime,
    max_memory_usage: 5368709120,
  }
