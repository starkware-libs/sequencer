// This file should contain the test instances for the deprecated structs. Usually, using the
// auto_impl_get_test_instance macro.
use apollo_test_utils::{auto_impl_get_test_instance, GetTestInstance};
use starknet_api::block::{BlockHash, BlockNumber, BlockTimestamp, GasPricePerToken};
use starknet_api::core::{
    EventCommitment,
    GlobalRoot,
    ReceiptCommitment,
    SequencerContractAddress,
    StateDiffCommitment,
    TransactionCommitment,
};
use starknet_api::data_availability::L1DataAvailabilityMode;
use starknet_api::execution_resources::GasAmount;

use crate::deprecated::migrations::StorageBlockHeaderV0;

auto_impl_get_test_instance! {
    pub struct StorageBlockHeaderV0 {
        pub block_hash: BlockHash,
        pub parent_hash: BlockHash,
        pub block_number: BlockNumber,
        pub l1_gas_price: GasPricePerToken,
        pub l1_data_gas_price: GasPricePerToken,
        pub l2_gas_price: GasPricePerToken,
        pub l2_gas_consumed: GasAmount,
        pub next_l2_gas_price: u64,
        pub state_root: GlobalRoot,
        pub sequencer: SequencerContractAddress,
        pub timestamp: BlockTimestamp,
        pub l1_da_mode: L1DataAvailabilityMode,
        pub state_diff_commitment: Option<StateDiffCommitment>,
        pub transaction_commitment: Option<TransactionCommitment>,
        pub event_commitment: Option<EventCommitment>,
        pub receipt_commitment: Option<ReceiptCommitment>,
        pub state_diff_length: Option<usize>,
        pub n_transactions: usize,
        pub n_events: usize,
    }
}
