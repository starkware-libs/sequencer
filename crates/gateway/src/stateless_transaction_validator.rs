use starknet_api::external_transaction::{
    ExternalDeclareTransaction, ExternalDeployAccountTransaction, ExternalInvokeTransaction,
    ExternalTransaction,
};
use starknet_api::transaction::{Resource, ResourceBoundsMapping};

use crate::errors::{TransactionValidatorError, TransactionValidatorResult};

#[cfg(test)]
#[path = "stateless_transaction_validator_test.rs"]
mod transaction_validator_test;

#[derive(Default)]
pub struct StatelessTransactionValidatorConfig {
    // If true, validates that the resource bounds are not zero.
    pub validate_non_zero_l1_gas_fee: bool,
    pub validate_non_zero_l2_gas_fee: bool,
}

pub struct StatelessTransactionValidator {
    pub config: StatelessTransactionValidatorConfig,
}

impl StatelessTransactionValidator {
    pub fn validate(&self, tx: &ExternalTransaction) -> TransactionValidatorResult<()> {
        // TODO(Arni, 1/5/2024): Add a mechanism that validate the sender address is not blocked.
        // TODO(Arni, 1/5/2024): Validate transaction version.
        // TODO(Arni, 4/4/2024): Validate tx signature and calldata are not too long.

        self.validate_fee(tx)?;

        Ok(())
    }

    fn validate_fee(&self, tx: &ExternalTransaction) -> TransactionValidatorResult<()> {
        let resource_bounds_mapping = match tx {
            ExternalTransaction::Declare(ExternalDeclareTransaction::V3(tx)) => &tx.resource_bounds,
            ExternalTransaction::DeployAccount(ExternalDeployAccountTransaction::V3(tx)) => {
                &tx.resource_bounds
            }
            ExternalTransaction::Invoke(ExternalInvokeTransaction::V3(tx)) => &tx.resource_bounds,
        };

        if self.config.validate_non_zero_l1_gas_fee {
            validate_resource_bounds(resource_bounds_mapping, Resource::L1Gas)?;
        }
        if self.config.validate_non_zero_l2_gas_fee {
            validate_resource_bounds(resource_bounds_mapping, Resource::L2Gas)?;
        }

        Ok(())
    }
}

// Utilities.

fn validate_resource_bounds(
    resource_bounds_mapping: &ResourceBoundsMapping,
    resource: Resource,
) -> TransactionValidatorResult<()> {
    if let Some(resource_bounds) = resource_bounds_mapping.0.get(&resource) {
        if resource_bounds.max_amount == 0 || resource_bounds.max_price_per_unit == 0 {
            return Err(TransactionValidatorError::ZeroFee {
                resource,
                resource_bounds: *resource_bounds,
            });
        }
    } else {
        return Err(TransactionValidatorError::MissingResource { resource });
    }

    Ok(())
}
