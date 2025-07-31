use cairo_vm::hint_processor::builtin_hint_processor::dict_hint_utils::DICT_ACCESS_SIZE;
use cairo_vm::types::relocatable::Relocatable;
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::felt;

use super::{FullStateDiffWriter, StateEntry};
use crate::hints::hint_implementation::aggregator_utils::ToMaybeRelocatables;
use crate::io::os_output_types::{FullContractChanges, FullContractStorageUpdate};
#[test]
fn test_write_contract_changes() {
    let contract2_first_changes = FullContractChanges {
        addr: ContractAddress::from(2u8),
        prev_nonce: Nonce(felt!(200u16)),
        new_nonce: Nonce(felt!(300u16)),
        prev_class_hash: ClassHash(felt!(500u16)),
        new_class_hash: ClassHash(felt!(600u16)),
        storage_changes: vec![
            FullContractStorageUpdate {
                key: 100u128.into(),
                prev_value: felt!(200u16),
                new_value: felt!(300u16),
            },
            FullContractStorageUpdate {
                key: 200u128.into(),
                prev_value: felt!(300u16),
                new_value: felt!(400u16),
            },
        ],
    };

    let contract1_first_changes = FullContractChanges {
        addr: ContractAddress::from(1u8),
        prev_nonce: Nonce(felt!(2u16)),
        new_nonce: Nonce(felt!(3u16)),
        prev_class_hash: ClassHash(felt!(4u16)),
        new_class_hash: ClassHash(felt!(4u16)),
        storage_changes: vec![FullContractStorageUpdate {
            key: 1u128.into(),
            prev_value: felt!(2u16),
            new_value: felt!(3u16),
        }],
    };

    let contract2_second_changes = FullContractChanges {
        addr: ContractAddress::from(2u8),
        prev_nonce: Nonce(felt!(300u16)),
        new_nonce: Nonce(felt!(400u16)),
        prev_class_hash: ClassHash(felt!(600u16)),
        new_class_hash: ClassHash(felt!(600u16)),
        storage_changes: vec![FullContractStorageUpdate {
            key: 100u128.into(),
            prev_value: felt!(300u16),
            new_value: felt!(400u16),
        }],
    };

    // Represents a first block "state diff".
    let first_changes = vec![contract2_first_changes, contract1_first_changes];
    // Represents a second block "state diff".
    let second_changes = vec![contract2_second_changes];

    let mut vm = VirtualMachine::new(false, false);
    let mut state_diff_writer = FullStateDiffWriter::new(&mut vm);
    let state_diff_start_ptr = state_diff_writer.get_state_dict_ptr();
    for changes in [&first_changes, &second_changes] {
        state_diff_writer.write_contract_changes(changes, &mut vm).unwrap();
    }

    // Test values
    let n_changes = 3;
    let state_dict =
        vm.get_continuous_range(state_diff_start_ptr, n_changes * DICT_ACCESS_SIZE).unwrap();

    for (i, changes) in first_changes.iter().chain(second_changes.iter()).enumerate() {
        let state_dict_index = i * n_changes;
        assert_eq!(state_dict[state_dict_index], changes.addr.0.key().into());

        let prev_stat_addr = &state_dict[state_dict_index + 1];
        // A state is a triplet of the form (class_hash, storage_dict_ptr, nonce).
        let prev_state = vm
            .get_continuous_range(prev_stat_addr.try_into().unwrap(), StateEntry::size())
            .unwrap();
        assert_eq!(prev_state[0], changes.prev_class_hash.0.into());
        let prev_storage_addr: Relocatable = (&prev_state[1]).try_into().unwrap();
        assert_eq!(prev_state[2], changes.prev_nonce.0.into());

        let new_stat_addr = &state_dict[state_dict_index + 2];
        let new_state =
            vm.get_continuous_range(new_stat_addr.try_into().unwrap(), StateEntry::size()).unwrap();
        assert_eq!(new_state[0], changes.new_class_hash.0.into());
        let new_storage_addr: Relocatable = (&new_state[1]).try_into().unwrap();
        assert_eq!(new_state[2], changes.new_nonce.0.into());

        // Verify the storage dict.
        let n_storage_mem_cells = changes.storage_changes.len() * DICT_ACCESS_SIZE;
        assert_eq!((prev_storage_addr + n_storage_mem_cells).unwrap(), new_storage_addr);
        let storage_changes = vm
            .get_continuous_range(prev_storage_addr.try_into().unwrap(), n_storage_mem_cells)
            .unwrap();
        assert_eq!(
            storage_changes,
            changes
                .storage_changes
                .iter()
                .map(|changes| changes.to_maybe_relocatables())
                .flatten()
                .collect::<Vec<_>>()
        );
    }
}
