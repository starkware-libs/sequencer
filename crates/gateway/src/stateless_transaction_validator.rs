use starknet_api::block::GasPrice;
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::execution_resources::GasAmount;
use starknet_api::rpc_transaction::{
    RpcDeclareTransaction,
    RpcDeployAccountTransaction,
    RpcInvokeTransaction,
    RpcTransaction,
};
use starknet_api::state::EntryPoint;
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
    pub fn validate(&self, tx: &RpcTransaction) -> StatelessTransactionValidatorResult<()> {
        // TODO(Arni, 1/5/2024): Add a mechanism that validate the sender address is not blocked.
        // TODO(Arni, 1/5/2024): Validate transaction version.

        Self::validate_contract_address(tx)?;
        Self::validate_empty_account_deployment_data(tx)?;
        Self::validate_empty_paymaster_data(tx)?;
        self.validate_resource_bounds(tx)?;
        self.validate_tx_size(tx)?;
        self.validate_nonce_data_availability_mode(tx)?;

        if let RpcTransaction::Declare(declare_tx) = tx {
            self.validate_declare_tx(declare_tx)?;
        }
        Ok(())
    }

    fn validate_resource_bounds(
        &self,
        tx: &RpcTransaction,
    ) -> StatelessTransactionValidatorResult<()> {
        let resource_bounds_mapping = tx.resource_bounds();

        if self.config.validate_non_zero_l1_gas_fee {
            validate_resource_is_non_zero(resource_bounds_mapping, Resource::L1Gas)?;
        }
        if self.config.validate_non_zero_l2_gas_fee {
            validate_resource_is_non_zero(resource_bounds_mapping, Resource::L2Gas)?;
        }

        Ok(())
    }

    fn validate_contract_address(tx: &RpcTransaction) -> StatelessTransactionValidatorResult<()> {
        let sender_address = match tx {
            RpcTransaction::Declare(RpcDeclareTransaction::V3(tx)) => tx.sender_address,
            RpcTransaction::DeployAccount(_) => return Ok(()),
            RpcTransaction::Invoke(RpcInvokeTransaction::V3(tx)) => tx.sender_address,
        };

        Ok(sender_address.validate()?)
    }

    /// The Starknet OS enforces that the deployer data is empty. We add this validation here at the
    /// gateway to prevent transactions from reverting.
    fn validate_empty_account_deployment_data(
        tx: &RpcTransaction,
    ) -> StatelessTransactionValidatorResult<()> {
        let account_deployment_data = match tx {
            RpcTransaction::DeployAccount(_) => return Ok(()),
            RpcTransaction::Declare(RpcDeclareTransaction::V3(tx)) => &tx.account_deployment_data,
            RpcTransaction::Invoke(RpcInvokeTransaction::V3(tx)) => &tx.account_deployment_data,
        };

        if account_deployment_data.is_empty() {
            Ok(())
        } else {
            Err(StatelessTransactionValidatorError::NonEmptyField {
                field_name: "account_deployment_data".to_string(),
            })
        }
    }

    /// The Starknet OS enforces that the paymaster data is empty. We add this validation here at
    /// the gateway to prevent transactions from reverting.
    fn validate_empty_paymaster_data(
        tx: &RpcTransaction,
    ) -> StatelessTransactionValidatorResult<()> {
        let paymaster_data = match tx {
            RpcTransaction::DeployAccount(RpcDeployAccountTransaction::V3(tx)) => {
                &tx.paymaster_data
            }
            RpcTransaction::Declare(RpcDeclareTransaction::V3(tx)) => &tx.paymaster_data,
            RpcTransaction::Invoke(RpcInvokeTransaction::V3(tx)) => &tx.paymaster_data,
        };

        if paymaster_data.is_empty() {
            Ok(())
        } else {
            Err(StatelessTransactionValidatorError::NonEmptyField {
                field_name: "paymaster_data".to_string(),
            })
        }
    }

    fn validate_tx_size(&self, tx: &RpcTransaction) -> StatelessTransactionValidatorResult<()> {
        self.validate_tx_calldata_size(tx)?;
        self.validate_tx_signature_size(tx)?;

        Ok(())
    }

    fn validate_tx_calldata_size(
        &self,
        tx: &RpcTransaction,
    ) -> StatelessTransactionValidatorResult<()> {
        let calldata = match tx {
            RpcTransaction::Declare(_) => {
                // Declare transaction has no calldata.
                return Ok(());
            }
            RpcTransaction::DeployAccount(RpcDeployAccountTransaction::V3(tx)) => {
                &tx.constructor_calldata
            }
            RpcTransaction::Invoke(RpcInvokeTransaction::V3(tx)) => &tx.calldata,
        };

        let calldata_length = calldata.0.len();
        if calldata_length > self.config.max_calldata_length {
            return Err(StatelessTransactionValidatorError::CalldataTooLong {
                calldata_length,
                max_calldata_length: self.config.max_calldata_length,
            });
        }

        Ok(())
    }

    fn validate_tx_signature_size(
        &self,
        tx: &RpcTransaction,
    ) -> StatelessTransactionValidatorResult<()> {
        let signature = tx.signature();

        let signature_length = signature.0.len();
        if signature_length > self.config.max_signature_length {
            return Err(StatelessTransactionValidatorError::SignatureTooLong {
                signature_length,
                max_signature_length: self.config.max_signature_length,
            });
        }

        Ok(())
    }

    /// The Starknet OS enforces that the nonce data availability mode is L1. We add this validation
    /// here at the gateway to prevent transactions from reverting.
    fn validate_nonce_data_availability_mode(
        &self,
        tx: &RpcTransaction,
    ) -> StatelessTransactionValidatorResult<()> {
        let expected_da_mode = DataAvailabilityMode::L1;
        let da_mode = *tx.nonce_data_availability_mode();
        if da_mode != expected_da_mode {
            return Err(StatelessTransactionValidatorError::NonceDataAvailabilityMode);
        };

        Ok(())
    }

    fn validate_declare_tx(
        &self,
        declare_tx: &RpcDeclareTransaction,
    ) -> StatelessTransactionValidatorResult<()> {
        let contract_class = match declare_tx {
            RpcDeclareTransaction::V3(tx) => &tx.contract_class,
        };
        self.validate_sierra_version(&contract_class.sierra_program)?;
        self.validate_class_length(contract_class)?;
        self.validate_entry_points_sorted_and_unique(contract_class)?;
        Ok(())
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
        contract_class: &starknet_api::rpc_transaction::ContractClass,
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
        contract_class: &starknet_api::rpc_transaction::ContractClass,
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
