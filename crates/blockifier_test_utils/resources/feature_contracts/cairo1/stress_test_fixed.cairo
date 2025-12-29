#[starknet::contract]
mod StressTest {
    use starknet::storage::map::{StorageMapReadAccess, StorageMapWriteAccess};
    use starknet::storage::{
        Map, StoragePathEntry, StoragePointerReadAccess, StoragePointerWriteAccess,
    };

    /// How this call chooses its storage region.
    #[derive(Drop, Serde)]
    enum RegionSelection {
        /// Shared region where multiple calls can collide (using same group ID).
        Shared: u64,
        /// Each call gets its own region, using an auto-incremented ID.
        #[default]
        UniquePerCall,
    }

    /// How write operations choose which slot to write into.
    #[derive(Drop, Serde)]
    enum SlotStrategy {
        /// Writes wrap over a limited number of slots (controlled collision).
        CyclicSlots: u64,
        #[default]
        /// Each write uses a different slot (maximal StateDiff).
        OneSlotPerWrite,
    }

    #[starknet::interface]
    trait IStressTest<TContractState> {
        fn produce_large_state_diff(
            ref self: TContractState,
            total_writes: u64,
            slot_strategy: SlotStrategy,
            region: RegionSelection,
        );
        fn process_large_calldata(self: @TContractState, payload: Span<felt252>);
    }

    #[storage]
    struct Storage {
        /// Key: (group_id, call_id, slot_i).
        stress_test_storage: Map<(u64, u64, u64), u64>,
        /// Counter for generating UniquePerCall region IDs.
        unique_region_counter: u64,
    }

    #[abi(embed_v0)]
    impl StressTestImpl of IStressTest<ContractState> {
        /// Produces a controlled StateDiff for stress-testing.
        ///
        /// - `total_writes`: total number of storage writes this call performs.
        /// - `slot_strategy`:
        ///       OneSlotPerWrite → every write uses a different slot.
        ///       CyclicSlots(k) → writes wrap over k slots.
        /// - `region`:
        ///       UniquePerCall → this call gets a unique auto-incremented region id.
        ///       Shared(id) → all calls using the same id share the same region.
        fn produce_large_state_diff(
            ref self: ContractState,
            total_writes: u64,
            slot_strategy: SlotStrategy,
            region: RegionSelection,
        ) {
            // Decide how many slots to cycle over.
            let slot_span: u64 = match slot_strategy {
                SlotStrategy::CyclicSlots(slot_limit) => {
                    assert(slot_limit > 0, 'slot_limit_cannot_be_zero');
                    assert(slot_limit <= total_writes, 'slot_limit_exceeds_writes');
                    slot_limit
                },
                SlotStrategy::OneSlotPerWrite => { total_writes },
            };

            let (group_id, call_id) = self._resolve_region_keys(region);

            // Perform writes.
            let mut slot_i: u64 = 0;
            for write_sequence_number in 0_u64..total_writes {
                if slot_i >= slot_span {
                    slot_i = 0;
                }
                let storage_key_tuple = (group_id, call_id, slot_i);
                // Offset by +1 because writing 0 would be a no-op (0 is the default value).
                self.stress_test_storage.write(storage_key_tuple, write_sequence_number + 1);
                slot_i += 1;
            }
        }

        /// Entry point for stressing large calldata handling.
        fn process_large_calldata(self: @ContractState, payload: Span<felt252>) {
            // TODO(AvivG): Is this necessary?
            // Force compiler to process the calldata by accessing its length.
            let _calldata_length = payload.len();
        }
    }

    #[generate_trait]
    impl InternalFunctions of InternalFunctionsTrait {
        /// Converts RegionSelection to storage-compatible key components.
        /// Returns (group_id, call_id) for organizing storage writes.
        fn _resolve_region_keys(
            ref self: ContractState, access_pattern: RegionSelection,
        ) -> (u64, u64) {
            match access_pattern {
                // Shift shared group IDs by 1 to avoid collision with UniquePerCall.
                RegionSelection::Shared(group_id) => { (group_id + 1, 0_u64) },
                RegionSelection::UniquePerCall => {
                    let current_call_count = self.unique_region_counter.read();
                    self.unique_region_counter.write(current_call_count + 1_u64);
                    (0_u64, current_call_count)
                },
            }
        }
    }
}
