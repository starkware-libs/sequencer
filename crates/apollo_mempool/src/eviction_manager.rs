use std::collections::{BTreeMap, HashMap};

use starknet_api::core::{ContractAddress, Nonce};

use crate::mempool::TransactionReference;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct EvictionManager {
    pub suspended: HashMap<ContractAddress, BTreeMap<Nonce, TransactionReference>>,
}

impl EvictionManager {
    pub fn new() -> Self {
        EvictionManager { suspended: HashMap::new() }
    }

    pub fn add_tx(&mut self, tx_ref: TransactionReference, account_nonce: Nonce) {
        if tx_ref.nonce > account_nonce {
            self.suspended.entry(tx_ref.address).or_default().insert(tx_ref.nonce, tx_ref);
        } else {
            self.suspended.remove(&tx_ref.address);
        }
    }

    pub fn update_account_nonce(&mut self, address: ContractAddress, new_nonce: Nonce) {
        if let Some(txs) = self.suspended.get_mut(&address) {
            txs.retain(|&nonce, _| nonce > new_nonce);
            if txs.is_empty() {
                self.suspended.remove(&address);
            }
        }
    }
}
