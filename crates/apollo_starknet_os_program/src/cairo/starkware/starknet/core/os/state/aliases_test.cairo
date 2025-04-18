from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.dict import DictAccess
from starkware.cairo.common.find_element import find_element
from starkware.cairo.common.squash_dict import squash_dict
from starkware.starknet.core.os.constants import ALIAS_CONTRACT_ADDRESS
from starkware.starknet.core.os.state.aliases import (
    ALIAS_COUNTER_STORAGE_KEY,
    INITIAL_AVAILABLE_ALIAS,
    MAX_NON_COMPRESSED_CONTRACT_ADDRESS,
    Aliases,
    allocate_aliases,
    get_alias,
    get_full_contract_state_diff,
    get_next_available_alias,
    maybe_allocate_alias_for_key,
    replace_aliases_and_serialize_full_contract_state_diff,
)
from starkware.starknet.core.os.state.commitment import StateEntry
from starkware.starknet.core.os.state.state import allocate_aliases_and_squash_state_changes

func allocate_alias_for_keys{
    range_check_ptr,
    aliases_storage_updates: DictAccess*,
    aliases_storage_updates_start: DictAccess*,
    next_available_alias: felt,
}(n_keys: felt, keys: felt*) {
    if (n_keys == 0) {
        return ();
    }
    maybe_allocate_alias_for_key(key=[keys]);
    return allocate_alias_for_keys(n_keys=n_keys - 1, keys=keys + 1);
}

// Gets and writes the aliases of `keys` into `res`.
func get_aliases{range_check_ptr}(aliases: Aliases, n_keys: felt, keys: felt*, res: felt*) {
    if (n_keys == 0) {
        return ();
    }
    let alias = get_alias(aliases=aliases, key=keys[0]);
    assert res[0] = alias;
    return get_aliases(aliases=aliases, n_keys=n_keys - 1, keys=&keys[1], res=&res[1]);
}

// Allocates aliases for the given keys.
// Returns the aliases dict, and a list of aliases corresponding to the given keys.
func allocate_alias_for_keys_and_replace{range_check_ptr}(n_keys: felt, keys: felt*) -> (
    aliases: Aliases, alias_per_key: felt*
) {
    alloc_locals;
    let (aliases_storage_updates_start: DictAccess*) = alloc();
    local aliases_storage_updates: DictAccess* = aliases_storage_updates_start;

    let next_available_alias = get_next_available_alias{
        aliases_storage_updates=aliases_storage_updates
    }();
    local prev_available_alias: felt = next_available_alias;
    with next_available_alias, aliases_storage_updates, aliases_storage_updates_start {
        allocate_alias_for_keys(n_keys=n_keys, keys=keys);
    }

    // Update once the counter.
    assert aliases_storage_updates[0] = DictAccess(
        key=ALIAS_COUNTER_STORAGE_KEY,
        prev_value=prev_available_alias,
        new_value=next_available_alias,
    );
    let aliases_storage_updates = &aliases_storage_updates[1];
    let (squashed_start: DictAccess*) = alloc();
    let (squashed_end) = squash_dict(
        aliases_storage_updates_start, aliases_storage_updates, squashed_start
    );

    let aliases = Aliases(
        len=(squashed_end - squashed_start) / DictAccess.SIZE, ptr=squashed_start
    );
    let (alias_per_key: felt*) = alloc();
    get_aliases(aliases=aliases, n_keys=n_keys, keys=keys, res=alias_per_key);

    return (aliases=aliases, alias_per_key=alias_per_key);
}

// Allocates aliases for a given state diff.
// Returns:
// * The squashed storage updates of the alias contract, containing all state diff aliases.
// * full contract state diff.
// * The full contract state diff with replaced aliases.
func allocate_aliases_and_replace{range_check_ptr}(
    n_contracts: felt, contract_state_changes: DictAccess*
) -> (
    aliases_storage_ptr: DictAccess*,
    contract_state_diff: felt*,
    contract_state_diff_with_aliases: felt*,
) {
    alloc_locals;
    %{ state_update_pointers = None %}
    let (
        n_squashed_contracts, squashed_contract_state_changes
    ) = allocate_aliases_and_squash_state_changes(
        contract_state_changes_start=contract_state_changes,
        contract_state_changes_end=&contract_state_changes[n_contracts],
    );
    let (aliases_entry: DictAccess*) = find_element(
        array_ptr=squashed_contract_state_changes,
        elm_size=DictAccess.SIZE,
        n_elms=n_squashed_contracts,
        key=ALIAS_CONTRACT_ADDRESS,
    );
    let aliases_state_entry = cast(aliases_entry.prev_value, StateEntry*);

    let (contract_state_diff_with_aliases: felt*) = alloc();
    let res = contract_state_diff_with_aliases;
    with res {
        replace_aliases_and_serialize_full_contract_state_diff(
            n_contracts=n_squashed_contracts, contract_state_changes=squashed_contract_state_changes
        );
    }
    let contract_state_diff = get_full_contract_state_diff(
        n_contracts=n_squashed_contracts, contract_state_changes=squashed_contract_state_changes
    );
    return (
        aliases_storage_ptr=aliases_state_entry.storage_ptr,
        contract_state_diff=contract_state_diff,
        contract_state_diff_with_aliases=contract_state_diff_with_aliases,
    );
}

func test_constants(
    max_non_compressed_contract_address: felt,
    alias_counter_storage_key: felt,
    initial_available_alias: felt,
    alias_contract_address: felt,
) {
    assert max_non_compressed_contract_address = MAX_NON_COMPRESSED_CONTRACT_ADDRESS;
    assert alias_counter_storage_key = ALIAS_COUNTER_STORAGE_KEY;
    assert initial_available_alias = INITIAL_AVAILABLE_ALIAS;
    assert alias_contract_address = ALIAS_CONTRACT_ADDRESS;
    return ();
}
