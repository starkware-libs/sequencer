local defaults = import 'lib/defaults.libsonnet';
function(chain_params)
  {
    audited_libfuncs_only: chain_params.audited_libfuncs_only,
    max_bytecode_size: chain_params.max_bytecode_size,
    max_cpu_time: defaults.MAX_CPU_TIME,
    max_memory_usage: 5368709120,
  }
