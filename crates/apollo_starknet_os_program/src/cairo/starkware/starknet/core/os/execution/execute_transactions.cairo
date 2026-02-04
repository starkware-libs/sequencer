from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.cairo_builtins import (
    BitwiseBuiltin,
    EcOpBuiltin,
    HashBuiltin,
    KeccakBuiltin,
    ModBuiltin,
    PoseidonBuiltin,
)
from starkware.cairo.common.cairo_sha256.sha256_utils import finalize_sha256
from starkware.cairo.common.dict_access import DictAccess
from starkware.cairo.common.registers import get_fp_and_pc
from starkware.cairo.common.sha256_state import Sha256ProcessBlock
from starkware.starknet.builtins.segment_arena.segment_arena import new_arena
from starkware.starknet.core.os.block_context import BlockContext
from starkware.starknet.core.os.builtins import (
    BuiltinPointers,
    NonSelectableBuiltins,
    SelectableBuiltins,
)
from starkware.starknet.core.os.execution.execute_transactions_inner import (
    execute_transactions_inner,
)
from starkware.starknet.core.os.output import OsCarriedOutputs

func execute_transactions{
    pedersen_ptr: HashBuiltin*,
    range_check_ptr,
    ecdsa_ptr,
    bitwise_ptr: BitwiseBuiltin*,
    ec_op_ptr: EcOpBuiltin*,
    keccak_ptr: KeccakBuiltin*,
    poseidon_ptr: PoseidonBuiltin*,
    range_check96_ptr: felt*,
    add_mod_ptr: ModBuiltin*,
    mul_mod_ptr: ModBuiltin*,
    contract_state_changes: DictAccess*,
    contract_class_changes: DictAccess*,
    outputs: OsCarriedOutputs*,
    txs_range_check_ptr,
}(block_context: BlockContext*) {
    alloc_locals;

    // Prepare builtin pointers.
    let segment_arena_ptr = new_arena();
    let (sha256_ptr: Sha256ProcessBlock*) = alloc();

    let (__fp__, _) = get_fp_and_pc();
    local local_builtin_ptrs: BuiltinPointers = BuiltinPointers(
        selectable=SelectableBuiltins(
            pedersen=pedersen_ptr,
            range_check=txs_range_check_ptr,
            ecdsa=ecdsa_ptr,
            bitwise=bitwise_ptr,
            ec_op=ec_op_ptr,
            poseidon=poseidon_ptr,
            segment_arena=segment_arena_ptr,
            range_check96=range_check96_ptr,
            add_mod=add_mod_ptr,
            mul_mod=mul_mod_ptr,
        ),
        non_selectable=NonSelectableBuiltins(keccak=keccak_ptr, sha256=sha256_ptr),
    );

    let builtin_ptrs = &local_builtin_ptrs;
    let sha256_ptr_start = builtin_ptrs.non_selectable.sha256;

    // Execute transactions.
    local n_txs = nondet %{ len(block_input.transactions) %};
    %{
        vm_enter_scope({
            '__deprecated_class_hashes': __deprecated_class_hashes,
            'transactions': iter(block_input.transactions),
            'component_hashes': block_input.declared_class_hash_to_component_hashes,
            'execution_helper': execution_helper,
            'deprecated_syscall_handler': deprecated_syscall_handler,
            'syscall_handler': syscall_handler,
             '__dict_manager': __dict_manager,
        })
    %}
    execute_transactions_inner{
        builtin_ptrs=builtin_ptrs,
        contract_state_changes=contract_state_changes,
        contract_class_changes=contract_class_changes,
    }(block_context=block_context, n_txs=n_txs);
    %{ vm_exit_scope() %}

    let selectable_builtins = &builtin_ptrs.selectable;
    let pedersen_ptr = selectable_builtins.pedersen;
    let ecdsa_ptr = selectable_builtins.ecdsa;
    let bitwise_ptr = selectable_builtins.bitwise;
    let ec_op_ptr = selectable_builtins.ec_op;
    let poseidon_ptr = selectable_builtins.poseidon;
    let range_check96_ptr = selectable_builtins.range_check96;
    let add_mod_ptr = selectable_builtins.add_mod;
    let mul_mod_ptr = selectable_builtins.mul_mod;
    let keccak_ptr = builtin_ptrs.non_selectable.keccak;

    let txs_range_check_ptr = selectable_builtins.range_check;

    // Fill holes in the rc96 segment.
    %{
        rc96_ptr = ids.range_check96_ptr
        segment_size = rc96_ptr.offset
        base = rc96_ptr - segment_size

        for i in range(segment_size):
            memory.setdefault(base + i, 0)
    %}

    // Finalize the sha256 segment.
    finalize_sha256(
        sha256_ptr_start=sha256_ptr_start, sha256_ptr_end=builtin_ptrs.non_selectable.sha256
    );

    return ();
}

// Inner function for execute_transactions.
// Arguments:
// block_context - a read-only context used for transaction execution.
// n_txs - the number of transactions to execute.
//
// Implicit arguments:
// range_check_ptr - a range check builtin, used and advanced by the OS, not the transactions.
// builtin_ptrs - a struct of builtin pointer that are going to be used by the
// executed transactions.
// The range-checks used internally by the transactions do not affect range_check_ptr.
// They are accounted for in builtin_ptrs.
