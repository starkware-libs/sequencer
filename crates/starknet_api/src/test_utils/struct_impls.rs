use crate::core::ChainId;
use crate::test_utils::CHAIN_ID_NAME;

impl ChainId {
    pub fn creatre_for_testing() -> Self {
        Self::Other(CHAIN_ID_NAME.to_string())
    }
}
