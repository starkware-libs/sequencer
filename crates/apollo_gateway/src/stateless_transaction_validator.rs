use std::fmt::Debug;

use starknet_api::block::GasPrice;
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::execution_resources::GasAmount;
use starknet_api::state::EntryPoint;
use starknet_api::stateless_transaction_validations::StatelessValidateTransactionFields as ValidateTransaction;
use starknet_api::transaction::fields::{AllResourceBounds, Resource};
use starknet_types_core::felt::Felt;
use tracing::{instrument, Level};

use crate::compiler_version::VersionId;
use crate::config::StatelessTransactionValidatorConfig;
use crate::errors::{StatelessTransactionValidatorError, StatelessTransactionValidatorResult};

#[cfg(test)]
#[path = "stateless_transaction_validator_test.rs"]
mod stateless_transaction_validator_test;

#[derive(Clone)]
pub struct StatelessTransactionValidator {
    pub config: StatelessTransactionValidatorConfig,
}

impl StatelessTransactionValidator {
    #[instrument(skip(self), level = Level::INFO, err)]
    pub fn validate<T: ValidateTransaction + Debug>(
        &self,
        tx: &T,
    ) -> StatelessTransactionValidatorResult<()> {
        // TODO(Arni, 1/5/2024): Add a mechanism that validate the sender address is not blocked.
        // TODO(Arni, 1/5/2024): Validate transaction version.

        tx.validate_contract_address()?;
        self.validate_empty_account_deployment_data(tx)?;
        self.validate_empty_paymaster_data(tx)?;
        self.validate_resource_bounds(tx)?;
        self.validate_tx_size(tx)?;
        self.validate_nonce_data_availability_mode(tx)?;
        self.validate_fee_data_availability_mode(tx)?;

        self.validate_declare_tx(tx)?;

        Ok(())
    }

    /// The Starknet OS enforces that the deployer data is empty. We add this validation here in the
    /// gateway to prevent transactions from failing the OS.
    fn validate_empty_account_deployment_data<T: ValidateTransaction>(
        &self,
        tx: &T,
    ) -> StatelessTransactionValidatorResult<()> {
        if tx.account_deployment_data_is_empty() {
            Ok(())
        } else {
            Err(StatelessTransactionValidatorError::NonEmptyField {
                field_name: "account_deployment_data".to_string(),
            })
        }
    }

    /// The Starknet OS enforces that the paymaster data is empty. We add this validation here in
    /// the gateway to prevent transactions from failing the OS.
    fn validate_empty_paymaster_data<T: ValidateTransaction>(
        &self,
        tx: &T,
    ) -> StatelessTransactionValidatorResult<()> {
        if tx.paymaster_data_is_empty() {
            Ok(())
        } else {
            Err(StatelessTransactionValidatorError::NonEmptyField {
                field_name: "paymaster_data".to_string(),
            })
        }
    }

    fn validate_resource_bounds<T: ValidateTransaction>(
        &self,
        tx: &T,
    ) -> StatelessTransactionValidatorResult<()> {
        let resource_bounds_mapping = tx.resource_bounds();

        if self.config.validate_non_zero_l1_gas_fee {
            validate_resource_is_non_zero(resource_bounds_mapping, Resource::L1Gas)?;
        }
        if self.config.validate_non_zero_l2_gas_fee {
            validate_resource_is_non_zero(resource_bounds_mapping, Resource::L2Gas)?;
        }
        if self.config.validate_non_zero_l1_data_gas_fee {
            validate_resource_is_non_zero(resource_bounds_mapping, Resource::L1DataGas)?;
        }

        Ok(())
    }

    fn validate_tx_size<T: ValidateTransaction>(
        &self,
        tx: &T,
    ) -> StatelessTransactionValidatorResult<()> {
        self.validate_tx_calldata_size(tx)?;
        self.validate_tx_signature_size(tx)?;

        Ok(())
    }

    fn validate_tx_calldata_size<T: ValidateTransaction>(
        &self,
        tx: &T,
    ) -> StatelessTransactionValidatorResult<()> {
        let Some(calldata_length) = tx.calldata_length() else {
            return Ok(());
        };

        if calldata_length > self.config.max_calldata_length {
            return Err(StatelessTransactionValidatorError::CalldataTooLong {
                calldata_length,
                max_calldata_length: self.config.max_calldata_length,
            });
        }

        Ok(())
    }

    fn validate_tx_signature_size<T: ValidateTransaction>(
        &self,
        tx: &T,
    ) -> StatelessTransactionValidatorResult<()> {
        let signature_length = tx.signature_length();

        if signature_length > self.config.max_signature_length {
            return Err(StatelessTransactionValidatorError::SignatureTooLong {
                signature_length,
                max_signature_length: self.config.max_signature_length,
            });
        }

        Ok(())
    }

    /// The Starknet OS enforces that the nonce data availability mode is L1. We add this validation
    /// here in the gateway to prevent transactions from failing the OS.
    fn validate_nonce_data_availability_mode<T: ValidateTransaction>(
        &self,
        tx: &T,
    ) -> StatelessTransactionValidatorResult<()> {
        let expected_da_mode = DataAvailabilityMode::L1;
        let da_mode = *tx.nonce_data_availability_mode();
        if da_mode != expected_da_mode {
            return Err(StatelessTransactionValidatorError::InvalidDataAvailabilityMode {
                field_name: "nonce".to_string(),
            });
        };

        Ok(())
    }

    /// The Starknet OS enforces that the fee data availability mode is L1. We add this validation
    /// here in the gateway to prevent transactions from failing the OS.
    fn validate_fee_data_availability_mode<T: ValidateTransaction>(
        &self,
        tx: &T,
    ) -> StatelessTransactionValidatorResult<()> {
        let expected_fee_mode = DataAvailabilityMode::L1;
        let fee_mode = *tx.fee_data_availability_mode();
        if fee_mode != expected_fee_mode {
            return Err(StatelessTransactionValidatorError::InvalidDataAvailabilityMode {
                field_name: "fee".to_string(),
            });
        };

        Ok(())
    }

    fn validate_declare_tx<T: ValidateTransaction>(
        &self,
        tx: &T,
    ) -> StatelessTransactionValidatorResult<()> {
        if !tx.is_declare() {
            return Ok(());
        }
        if let Some(contract_class) = tx.contract_class() {
            self.validate_sierra_version(&contract_class.sierra_program)?;
            self.validate_class_length(&contract_class)?;
            self.validate_entry_points_sorted_and_unique(&contract_class)?;
            Ok(())
        } else {
            Err(StatelessTransactionValidatorError::NonEmptyField {
                field_name: "contract_class".to_string(),
            })
        }
    }

    fn validate_sierra_version(
        &self,
        sierra_program: &[Felt],
    ) -> StatelessTransactionValidatorResult<()> {
        // Any patch version is valid. (i.e. when check version for upper bound, we ignore the Z
        // part in a version X.Y.Z).
        let mut max_sierra_version = self.config.max_sierra_version;
        max_sierra_version.0.patch = usize::MAX;

        let sierra_version = VersionId::from_sierra_program(sierra_program)?;
        if self.config.min_sierra_version <= sierra_version && sierra_version <= max_sierra_version
        {
            return Ok(());
        }

        Err(StatelessTransactionValidatorError::UnsupportedSierraVersion {
            version: sierra_version,
            min_version: self.config.min_sierra_version,
            max_version: self.config.max_sierra_version,
        })
    }

    fn validate_class_length(
        &self,
        contract_class: &starknet_api::state::SierraContractClass,
    ) -> StatelessTransactionValidatorResult<()> {
        let contract_class_object_size = serde_json::to_string(&contract_class)
            .expect("Unexpected error serializing contract class.")
            .len();
        if contract_class_object_size > self.config.max_contract_class_object_size {
            return Err(StatelessTransactionValidatorError::ContractClassObjectSizeTooLarge {
                contract_class_object_size,
                max_contract_class_object_size: self.config.max_contract_class_object_size,
            });
        }

        Ok(())
    }

    fn validate_entry_points_sorted_and_unique(
        &self,
        contract_class: &starknet_api::state::SierraContractClass,
    ) -> StatelessTransactionValidatorResult<()> {
        let is_sorted_unique = |entry_points: &[EntryPoint]| {
            entry_points.windows(2).all(|pair| pair[0].selector < pair[1].selector)
        };

        if is_sorted_unique(&contract_class.entry_points_by_type.constructor)
            && is_sorted_unique(&contract_class.entry_points_by_type.external)
            && is_sorted_unique(&contract_class.entry_points_by_type.l1handler)
        {
            return Ok(());
        }

        Err(StatelessTransactionValidatorError::EntryPointsNotUniquelySorted)
    }
}

fn validate_resource_is_non_zero(
    all_resource_bounds: &AllResourceBounds,
    resource: Resource,
) -> StatelessTransactionValidatorResult<()> {
    let resource_bounds = all_resource_bounds.get_bound(resource);
    if resource_bounds.max_amount == GasAmount(0)
        || resource_bounds.max_price_per_unit == GasPrice(0)
    {
        return Err(StatelessTransactionValidatorError::ZeroResourceBounds {
            resource,
            resource_bounds,
        });
    }

    Ok(())
}
