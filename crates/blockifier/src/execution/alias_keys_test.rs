use std::collections::HashMap;

use rstest::rstest;
use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use super::{n_charged_invoke_aliases, N_SAVED_CONTRACT_ADDRESSES, N_TRIVIAL_SELF_ALIASES};
use crate::state::cached_state::{CachedState, StateChanges, StateMaps, StorageEntry};
use crate::test_utils::dict_state_reader::DictStateReader;

fn initial_state() -> CachedState<DictStateReader> {
    CachedState::from(DictStateReader {
        storage_view: HashMap::from([(
            (ContractAddress::from(1000_u32), StorageKey::from(1001_u32)),
            Felt::from(1002),
        )]),
        address_to_class_hash: HashMap::from([(
            ContractAddress::from(2000_u32),
            ClassHash(Felt::from(2001)),
        )]),
        ..Default::default()
    })
}

fn assert_number_of_charged_changes(state_changes: StateChanges, charged_change: bool) {
    let state = initial_state();
    let n_charged_aliases = n_charged_invoke_aliases(&state, &state_changes).unwrap();
    assert_eq!(n_charged_aliases, if charged_change { 1 } else { 0 });
}

#[rstest]
#[case::empty(HashMap::new(), false)]
#[case::non_zero_to_non_zero(HashMap::from([((ContractAddress::from(1000_u32), StorageKey::from(1001_u32)), Felt::from(1003))]), false)]
#[case::non_zero_to_zero(HashMap::from([((ContractAddress::from(1000_u32), StorageKey::from(1001_u32)), Felt::ZERO)]), false)]
#[case::zero_to_non_zero(HashMap::from([((ContractAddress::from(1000_u32), StorageKey::from(1004_u32)), Felt::from(1005))]), true)]
#[case::low_key(HashMap::from([((ContractAddress::from(1000_u32), StorageKey::from(N_TRIVIAL_SELF_ALIASES - 1)), Felt::from(1005))]), false)]
#[case::low_address(HashMap::from([((ContractAddress::from(N_SAVED_CONTRACT_ADDRESSES - 1), StorageKey::from(1004_u32)), Felt::from(1005))]), false)]
fn test_charged_storage_changes(
    #[case] storage_changes: HashMap<StorageEntry, Felt>,
    #[case] charged_change: bool,
) {
    assert_number_of_charged_changes(
        StateChanges(StateMaps { storage: storage_changes, ..Default::default() }),
        charged_change,
    );
}

#[rstest]
#[case::empty(HashMap::new(), false)]
#[case::add_class_hash(HashMap::from([(ClassHash(3000.into()), true)]), true)]
#[case::remove_class_hash(HashMap::from([(ClassHash(3000.into()), false)]), false)]
#[case::low_key(HashMap::from([(ClassHash((N_TRIVIAL_SELF_ALIASES - 1).into()), false)]), false)]
fn test_charged_declared_classes(
    #[case] declared_contracts: HashMap<ClassHash, bool>,
    #[case] charged_change: bool,
) {
    assert_number_of_charged_changes(
        StateChanges(StateMaps { declared_contracts, ..Default::default() }),
        charged_change,
    );
}

#[rstest]
#[case::empty(HashMap::new(), false)]
#[case::new_contract(HashMap::from([(ContractAddress::from(3000_u32), ClassHash(3001.into()))]), true)]
#[case::replace_class(HashMap::from([(ContractAddress::from(2000_u32), ClassHash(2002.into()))]), false)]
#[case::low_key(HashMap::from([(ContractAddress::from(N_TRIVIAL_SELF_ALIASES - 1), ClassHash(3001.into()))]), false)]
fn test_charged_deployed_contracts(
    #[case] class_hashes: HashMap<ContractAddress, ClassHash>,
    #[case] charged_change: bool,
) {
    assert_number_of_charged_changes(
        StateChanges(StateMaps { class_hashes, ..Default::default() }),
        charged_change,
    );
}
