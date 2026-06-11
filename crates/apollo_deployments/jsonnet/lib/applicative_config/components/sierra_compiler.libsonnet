local defaults = import 'lib/defaults.libsonnet';
function(replacers)
  {
    audited_libfuncs_only: replacers['sierra_compiler_config.audited_libfuncs_only'],
    max_bytecode_size: replacers['sierra_compiler_config.max_bytecode_size'],
    max_cpu_time: defaults.MAX_CPU_TIME,
    max_memory_usage: 5368709120,
  }
