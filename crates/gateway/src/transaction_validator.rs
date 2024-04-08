use starknet_api::external_transaction::ExternalTransaction;

use crate::errors::TransactionValidatorResult;

#[cfg(test)]
#[path = "transaction_validator_test.rs"]
mod transaction_validator_test;

pub struct TransactionValidatorConfig {}

pub struct TransactionValidator {
    pub config: TransactionValidatorConfig,
}

impl TransactionValidator {
    pub fn validate(&self, _tx: ExternalTransaction) -> TransactionValidatorResult<()> {
        // TODO(Arni, 1/5/2024): Add a mechanism that validate the sender address is not blocked.
        // TODO(Arni, 1/5/2024): Validate transaction version.
        // TODO(Arni, 4/4/2024): Validate fee non zero.
        // TODO(Arni, 4/4/2024): Validate tx signature and calldata are not too long.

        Ok(())
    }
}
