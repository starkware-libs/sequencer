use std::collections::BTreeMap;

use cairo_vm::vm::runners::cairo_runner::ExecutionResources;

mod builtins_test;
mod call_contract;
mod constants;
mod deploy;
mod emit_event;
mod failure_format;
mod get_block_hash;
mod get_class_hash_at;
mod get_execution_info;
mod keccak;
mod library_call;
mod meta_tx;
mod out_of_gas;
mod replace_class;
mod secp;
mod send_message_to_l1;
mod sha256;
mod storage_read_write;

#[derive(Debug)]
pub struct DeterministicExecutionResources {
    pub n_steps: usize,
    pub n_memory_holes: usize,
    pub builtin_instance_counter: BTreeMap<String, usize>,
}

impl DeterministicExecutionResources {
    pub fn from(resources: &ExecutionResources) -> Self {
        Self {
            n_steps: resources.n_steps,
            n_memory_holes: resources.n_memory_holes,
            builtin_instance_counter: BTreeMap::from_iter(
                resources.builtin_instance_counter.iter().map(|(k, v)| (k.to_string(), *v)),
            ),
        }
    }
}
