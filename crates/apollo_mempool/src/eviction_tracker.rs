use std::collections::HashSet;

use starknet_api::core::ContractAddress;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct EvictionTracker {
    accounts_with_gap: HashSet<ContractAddress>,
}

impl EvictionTracker {
    pub fn new() -> Self {
        EvictionTracker { accounts_with_gap: HashSet::new() }
    }

    pub fn update(&mut self, address: ContractAddress, has_nonce_gap: bool) {
        if has_nonce_gap {
            self.accounts_with_gap.insert(address);
        } else {
            self.accounts_with_gap.remove(&address);
        }
    }

    #[cfg(test)]
    pub fn contains(&self, address: &ContractAddress) -> bool {
        self.accounts_with_gap.contains(address)
    }
}
