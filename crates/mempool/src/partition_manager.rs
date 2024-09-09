use starknet_api::core::{ContractAddress, Nonce};
use starknet_mempool_types::mempool_types::MempoolResult;

use crate::mempool::TransactionReference;

#[derive(Debug, Default)]
pub struct PartitionManager {}

impl PartitionManager {
    // TODO(Ayelet): Implement this function.
    pub fn insert(
        &mut self,
        _tx: TransactionReference,
        _account_nonce: Nonce,
    ) -> MempoolResult<()> {
        Ok(())
    }

    // TODO(Ayelet): Implement this function.
    pub fn align_with_current_state(&mut self, _address: ContractAddress, _nonce: Nonce) {}

    // TODO(Ayelet): Implement this function.
    pub fn remove(&mut self, _tx: &TransactionReference) {}

    // TODO(Ayelet): Implement this function.
    pub fn _update_gas_price_threshold(&mut self, _gas_price_threshold: u128) {}
}
