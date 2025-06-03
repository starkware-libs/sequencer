const COMPILED_CLASS_VERSION = 'COMPILED_CLASS_V1';

struct CompiledClassEntryPoint {
    // A field element that encodes the signature of the called function.
    selector: felt,
    // The offset of the instruction that should be called within the contract bytecode.
    offset: felt,
    // The number of builtins in 'builtin_list'.
    n_builtins: felt,
    // 'builtin_list' is a continuous memory segment containing the ASCII encoding of the (ordered)
    // builtins used by the function.
    builtin_list: felt*,
}

struct CompiledClass {
    compiled_class_version: felt,

    // The length and pointer to the external entry point table of the contract.
    n_external_functions: felt,
    external_functions: CompiledClassEntryPoint*,

    // The length and pointer to the L1 handler entry point table of the contract.
    n_l1_handlers: felt,
    l1_handlers: CompiledClassEntryPoint*,

    // The length and pointer to the constructor entry point table of the contract.
    n_constructors: felt,
    constructors: CompiledClassEntryPoint*,

    // The length and pointer of the bytecode.
    bytecode_length: felt,
    bytecode_ptr: felt*,
}

// A list entry that maps a hash to the corresponding contract classes.
struct CompiledClassFact {
    // The hash of the contract. This member should be first, so that we can lookup items
    // with the hash as key, using find_element().
    hash: felt,
    compiled_class: CompiledClass*,
}
