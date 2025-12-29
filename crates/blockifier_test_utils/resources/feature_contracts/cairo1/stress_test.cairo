use starknet::storage::map::{StorageMapReadAccess, StorageMapWriteAccess};
/// *To consider- writing in close cells vs far apart.
/// *Transfers from different ERC20s - will improve performance?

use starknet::storage::{Map, StoragePathEntry};

#[starknet::interface]
trait IStressTest<TContractState> {
    fn produce_large_state_diff(
        ref self: TContractState,
        n_writes: u64,
        slots_per_call: Option<u64>,
        overlap_group: Option<u64>,
    );
    fn process_large_calldata_for_stress(self: @TContractState, input_data: Span<felt252>);
}

#[starknet::contract]
mod StressTest {
    use starknet::storage::map::{StorageMapReadAccess, StorageMapWriteAccess};
    use starknet::storage::{
        Map, StoragePathEntry, StoragePointerReadAccess, StoragePointerWriteAccess,
    };

    #[storage]
    struct Storage {
        /// Storage for StateDiff generation:
        /// key = (group_id, call_number, slot_index)
        /// val = write index (i)
        diff_sink: Map<(u64, u64, u64), u64>,
        /// Counter for unique call regions.
        call_counter: u64,
        calldata_digest: felt252,
    }

    #[abi(embed_v0)]
    impl StressTestImpl of super::IStressTest<ContractState> {
        /// Produces a controlled StateDiff for infra stress-testing.
        ///
        /// - `n_writes`: total number of writes this call performs.
        /// - `slots_per_call`:
        ///       None → every write uses a different slot (maximal StateDiff).
        ///       Some(k) → writes wrap over k slots.
        /// - `overlap_group`:
        ///       None → this call gets its own unique region.
        ///       Some(id) → all calls using the same id collide on the same region.
        fn produce_large_state_diff(
            ref self: ContractState,
            n_writes: u64,
            slots_per_call: Option<u64>,
            overlap_group: Option<u64>,
        ) {
            let (group_id, call_number): (u64, u64) = self._get_call_region(overlap_group);

            // Determine number of distinct slots to use
            let total_slots: u64 = match slots_per_call {
                Option::None => n_writes,
                Option::Some(k) => {
                    assert(k > 0, 'slots_per_call=0');
                    assert(k <= n_writes, 'slots_per_call>n_writes');
                    k
                },
            };

            // Perform the writes
            let mut slot_index: u64 = 0;
            for i in 0_u64..n_writes {
                if slot_index >= total_slots {
                    slot_index = 0;
                }
                let key = (group_id, call_number, slot_index);
                self.diff_sink.entry(key).write(i);
                slot_index += 1;
            }
        }

        /// Entry point for stressing large calldata handling.
        fn process_large_calldata_for_stress(self: @ContractState, input_data: Span<felt252>) {
            // Question: are there actions required to avoid compiler optimizations?
            let _len = input_data.len(); // Use the input to prevent optimization
        }
    }

    #[generate_trait]
    impl InternalFunctions of InternalFunctionsTrait {
        /// Determines the call region ID and updates the global call counter if no overlap ID is
        /// provided.
        fn _get_call_region(ref self: ContractState, overlap_group: Option<u64>) -> (u64, u64) {
            match overlap_group {
                Option::Some(id) => {
                    // Use provided ID, treat as call 0 for overlapping with this group.
                    (id, 0_u64)
                },
                Option::None => {
                    let current_call_number = self.call_counter.read();
                    self.call_counter.write(current_call_number + 1_u64);

                    // Use 0 for group ID, and the current counter value as the unique call number -
                    // so that different calls will not collide.
                    (0_u64, current_call_number)
                },
            }
        }
    }
}
