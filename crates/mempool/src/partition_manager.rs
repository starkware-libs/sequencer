use std::collections::BTreeMap;
use std::sync::Arc;

use starknet_api::core::Nonce;

use crate::mempool::{GasPriceThreshold, TransactionReference};

// A queue holding the transaction that with nonces that match account nonces.
// Note: the derived comparison functionality considers the order guaranteed by the data structures
// used.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct PartitionManager {
    gas_price_threshold: Arc<GasPriceThreshold>,
    // Transactions with gas price above gas price threshold (sorted by tip).
    tx_mapping: BTreeMap<Nonce, TransactionReference>,
}

impl PartitionManager {
    /// Adds a transaction to the mempool, ensuring unique keys.
    /// Panics: if given a duplicate tx.
    pub fn _insert(&mut self, tx_reference: TransactionReference) {
        if tx_reference.get_l2_gas_price() < self.gas_price_threshold.get_gas_price_threshold() {
            self.tx_mapping.insert(tx_reference.nonce, tx_reference);
        }
    }

    pub fn _update_gas_price_threshold(&mut self, gas_price_threshold: Arc<GasPriceThreshold>) {
        let prev_threshold = self.gas_price_threshold.get_gas_price_threshold();
        self.gas_price_threshold = gas_price_threshold;
        println!(
            "MoNas: Inside PM: Updating gas price threshold to: {}, prev threshold: {}",
            self.gas_price_threshold.get_gas_price_threshold(),
            prev_threshold
        );
    }
}
