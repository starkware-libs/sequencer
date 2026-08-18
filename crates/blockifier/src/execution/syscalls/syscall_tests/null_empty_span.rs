use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_types_core::felt::Felt;

use crate::execution::deprecated_syscalls::deprecated_syscall_executor::DeprecatedSyscallExecutorBaseError;
use crate::execution::deprecated_syscalls::hint_processor::read_felt_array as deprecated_read_felt_array;
use crate::execution::syscalls::hint_processor::read_felt_array;
use crate::execution::syscalls::vm_syscall_utils::SyscallExecutorBaseError;

fn vm_with_span(start: MaybeRelocatable, end: MaybeRelocatable) -> (VirtualMachine, Relocatable) {
    vm_with_span_in(VirtualMachine::new(false, false), start, end)
}

fn vm_with_span_in(
    mut vm: VirtualMachine,
    start: MaybeRelocatable,
    end: MaybeRelocatable,
) -> (VirtualMachine, Relocatable) {
    let span_ptr = vm.add_memory_segment();
    vm.load_data(span_ptr, &[start, end]).unwrap();
    (vm, span_ptr)
}

#[test]
fn read_felt_array_accepts_empty_felt_span() {
    // STARKNET-96: a null empty span arrives as two equal felt-zero endpoints (`cast(0, felt*)`).
    let (vm, mut span_ptr) = vm_with_span(Felt::ZERO.into(), Felt::ZERO.into());

    let values = read_felt_array::<SyscallExecutorBaseError>(&vm, &mut span_ptr).unwrap();

    assert!(values.is_empty());
}

#[test]
fn read_felt_array_accepts_equal_nonzero_felt_span() {
    // Any two equal endpoints denote an empty array, regardless of the (non-relocatable) value.
    let (vm, mut span_ptr) = vm_with_span(Felt::ONE.into(), Felt::ONE.into());

    let values = read_felt_array::<SyscallExecutorBaseError>(&vm, &mut span_ptr).unwrap();

    assert!(values.is_empty());
}

#[test]
fn read_felt_array_accepts_real_empty_span() {
    // A real (segment-backed) empty span has start == end pointing at the same relocatable.
    let mut vm = VirtualMachine::new(false, false);
    let data_ptr = vm.add_memory_segment();
    let (vm, mut span_ptr) = vm_with_span_in(vm, data_ptr.into(), data_ptr.into());

    let values = read_felt_array::<SyscallExecutorBaseError>(&vm, &mut span_ptr).unwrap();

    assert!(values.is_empty());
}

#[test]
fn read_felt_array_rejects_mixed_null_and_pointer_span() {
    let mut vm = VirtualMachine::new(false, false);
    let data_ptr = vm.add_memory_segment();
    let span_ptr = vm.add_memory_segment();
    vm.load_data(span_ptr, &[Felt::ZERO.into(), data_ptr.into()]).unwrap();
    let mut span_ptr = span_ptr;

    assert!(read_felt_array::<SyscallExecutorBaseError>(&vm, &mut span_ptr).is_err());
}

// The deprecated (Cairo0) parser uses a `(size, data_ptr)` layout. STARKNET-96's null span,
// forwarded from a Cairo1 caller into a Cairo0 contract's syscall, arrives as `(0, felt-zero-ptr)`.
fn vm_with_deprecated_array(
    array_size: MaybeRelocatable,
    data_ptr: MaybeRelocatable,
) -> (VirtualMachine, Relocatable) {
    let mut vm = VirtualMachine::new(false, false);
    let array_ptr = vm.add_memory_segment();
    vm.load_data(array_ptr, &[array_size, data_ptr]).unwrap();
    (vm, array_ptr)
}

#[test]
fn deprecated_read_felt_array_accepts_null_empty_array() {
    let (vm, mut array_ptr) = vm_with_deprecated_array(Felt::ZERO.into(), Felt::ZERO.into());

    let values =
        deprecated_read_felt_array::<DeprecatedSyscallExecutorBaseError>(&vm, &mut array_ptr)
            .unwrap();

    assert!(values.is_empty());
}

#[test]
fn deprecated_read_felt_array_accepts_real_empty_array() {
    let mut vm = VirtualMachine::new(false, false);
    let data_ptr = vm.add_memory_segment();
    let array_ptr = vm.add_memory_segment();
    vm.load_data(array_ptr, &[Felt::ZERO.into(), data_ptr.into()]).unwrap();
    let mut array_ptr = array_ptr;

    let values =
        deprecated_read_felt_array::<DeprecatedSyscallExecutorBaseError>(&vm, &mut array_ptr)
            .unwrap();

    assert!(values.is_empty());
}
