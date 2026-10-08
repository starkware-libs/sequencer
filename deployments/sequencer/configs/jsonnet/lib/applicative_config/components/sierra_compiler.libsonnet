local defaults = import 'lib/defaults.libsonnet';
function(chain_params)
  {
    allowed_libfuncs_list: chain_params.allowed_libfuncs_list,
    max_bytecode_size: chain_params.max_bytecode_size,
    max_cpu_time: defaults.MAX_CPU_TIME,
    max_memory_usage: 5368709120,
  }
