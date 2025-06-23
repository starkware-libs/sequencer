// OS-related constants.
pub const L1_TO_L2_MSG_HEADER_SIZE: usize = 5;
pub const L2_TO_L1_MSG_HEADER_SIZE: usize = 3;
pub const CLASS_UPDATE_SIZE: usize = 1;
pub const N_STEPS_PER_SEGMENT_ARENA_BUILTIN: usize = 10;

// Starknet solidity contract-related constants.
pub const N_DEFAULT_TOPICS: usize = 1; // Events have one default topic.

// Excluding the default topic.
pub const LOG_MSG_TO_L1_N_TOPICS: usize = 2;
pub const CONSUMED_MSG_TO_L2_N_TOPICS: usize = 3;

// The headers include the payload size, so we need to add +1 since arrays are encoded with two
// additional parameters (offset and length) in solidity.
pub const LOG_MSG_TO_L1_ENCODED_DATA_SIZE: usize =
    (L2_TO_L1_MSG_HEADER_SIZE + 1) - LOG_MSG_TO_L1_N_TOPICS;
pub const CONSUMED_MSG_TO_L2_ENCODED_DATA_SIZE: usize =
    (L1_TO_L2_MSG_HEADER_SIZE + 1) - CONSUMED_MSG_TO_L2_N_TOPICS;

// Transaction resource names.
// TODO(Amos, 1/10/2024): Rename to l1_gas_weight.
pub const L1_GAS_USAGE: &str = "gas_weight";
pub const N_STEPS_RESOURCE: &str = "n_steps";
pub const N_EVENTS: &str = "n_events";
pub const MESSAGE_SEGMENT_LENGTH: &str = "message_segment_length";
pub const STATE_DIFF_SIZE: &str = "state_diff_size";
pub const N_MEMORY_HOLES: &str = "n_memory_holes";
pub const SIERRA_GAS: &str = "sierra_gas";
pub const N_TXS: &str = "n_txs";
pub const PROVING_GAS: &str = "proving_gas";

// Casm hash calculation-related constants.
pub const CAIRO0_ENTRY_POINT_STRUCT_SIZE: usize = 2;
pub const N_STEPS_PER_PEDERSEN: usize = 8;

// The block number -> block hash mapping is written for the current block number minus this number.
pub const STORED_BLOCK_HASH_BUFFER: u64 = 10;

// Maximum possible Sierra gas for a transaction to run with in Sierra mode.
// This limit is derived from the stack size limit when running natively.
pub const MAX_POSSIBLE_SIERRA_GAS: u64 = 3_500_000_000;
