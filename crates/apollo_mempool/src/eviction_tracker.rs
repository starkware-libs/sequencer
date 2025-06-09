use std::collections::HashSet;

use starknet_api::core::ContractAddress;

/// Suggests accounts to evict transactions from when the Mempool reaches capacity.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct EvictionTracker {
    // Tracks accounts with a nonce gap: the Mempool has higher-nonce transactions but lacks one
    // with the account’s current nonce.
    // NOTE: Delayed declares don’t count until they are inserted.
    accounts_with_gap: HashSet<ContractAddress>,
}

impl EvictionTracker {
    pub fn _new() -> Self {
        EvictionTracker { accounts_with_gap: HashSet::new() }
    }

    pub fn _update(&mut self, address: ContractAddress, has_nonce_gap: bool) {
        if has_nonce_gap {
            self.accounts_with_gap.insert(address);
        } else {
            self.accounts_with_gap.remove(&address);
        }
    }

    #[cfg(test)]
    pub fn _contains(&self, address: &ContractAddress) -> bool {
        self.accounts_with_gap.contains(address)
    }
}
