use starknet_api::core::{ContractAddress, Nonce};
use starknet_mempool_types::mempool_types::MempoolResult;

use crate::mempool::TransactionReference;

pub struct _PartitionManager {}

impl _PartitionManager {
    pub fn _insert(
        &mut self,
        _tx: TransactionReference,
        _account_nonce: Nonce,
    ) -> MempoolResult<()> {
        todo!()
    }

    /// Aligns the struct with the current state of a specific address and nonce updated in the
    /// Mempool flow, e.g., during a commit block.
    pub fn _align_with_current_state(&mut self, _address: ContractAddress, _nonce: Nonce) {}

    pub fn _remove(&mut self, _tx: &TransactionReference) {
        todo!()
    }

    pub fn _update_gas_price_threshold(&mut self, _gas_price_threshold: u128) {
        todo!()
    }
}
