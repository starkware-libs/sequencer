use blockifier::test_utils::dict_state_reader::DictStateReader;

use crate::hint_processor::execution_helper::OsExecutionHelper;
use crate::hint_processor::snos_hint_processor::{
    DeprecatedSyscallHintProcessor,
    SnosHintProcessor,
    SyscallHintProcessor,
};
use crate::io::os_input::StarknetOsInput;

pub fn get_snos_hint_processor_for_testing() -> SnosHintProcessor<DictStateReader> {
    let execution_helper =
        OsExecutionHelper::<DictStateReader>::new_for_testing(StarknetOsInput::default());

    let syscall_handler = SyscallHintProcessor {};
    let deprecated_syscall_handler = DeprecatedSyscallHintProcessor {};

    SnosHintProcessor::new(execution_helper, syscall_handler, deprecated_syscall_handler)
}
