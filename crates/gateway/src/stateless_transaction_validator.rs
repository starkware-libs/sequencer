use starknet_api::hash::StarkFelt;
use starknet_api::rpc_transaction::{
    RPCDeclareTransaction, RPCDeployAccountTransaction, RPCInvokeTransaction, RPCTransaction,
    ResourceBoundsMapping,
};
use starknet_api::transaction::Resource;

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
    pub fn validate(&self, tx: &RPCTransaction) -> StatelessTransactionValidatorResult<()> {
        // TODO(Arni, 1/5/2024): Add a mechanism that validate the sender address is not blocked.
        // TODO(Arni, 1/5/2024): Validate transaction version.

        self.validate_resource_bounds(tx)?;
        self.validate_tx_size(tx)?;

        if let RPCTransaction::Declare(declare_tx) = tx {
            self.validate_declare_tx(declare_tx)?;
        }
        Ok(())
    }

    fn validate_resource_bounds(
        &self,
        tx: &RPCTransaction,
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

    fn validate_tx_size(&self, tx: &RPCTransaction) -> StatelessTransactionValidatorResult<()> {
        self.validate_tx_calldata_size(tx)?;
        self.validate_tx_signature_size(tx)?;

        Ok(())
    }

    fn validate_tx_calldata_size(
        &self,
        tx: &RPCTransaction,
    ) -> StatelessTransactionValidatorResult<()> {
        let calldata = match tx {
            RPCTransaction::Declare(_) => {
                // Declare transaction has no calldata.
                return Ok(());
            }
            RPCTransaction::DeployAccount(RPCDeployAccountTransaction::V3(tx)) => {
                &tx.constructor_calldata
            }
            RPCTransaction::Invoke(RPCInvokeTransaction::V3(tx)) => &tx.calldata,
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
        tx: &RPCTransaction,
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

    fn validate_declare_tx(
        &self,
        declare_tx: &RPCDeclareTransaction,
    ) -> StatelessTransactionValidatorResult<()> {
        let contract_class = match declare_tx {
            RPCDeclareTransaction::V3(tx) => &tx.contract_class,
        };
        self.validate_sierra_version(&contract_class.sierra_program)?;
        self.validate_class_length(contract_class)
    }

    fn validate_sierra_version(
        &self,
        sierra_program: &[StarkFelt],
    ) -> StatelessTransactionValidatorResult<()> {
        // TODO(Arni): Validate the sierra version is supported.
        let _sierra_version = VersionId::from_sierra_program(sierra_program)?;

        Ok(())
    }

    fn validate_class_length(
        &self,
        contract_class: &starknet_api::rpc_transaction::ContractClass,
    ) -> StatelessTransactionValidatorResult<()> {
        let bytecode_size = contract_class.sierra_program.len();
        if bytecode_size > self.config.max_bytecode_size {
            return Err(StatelessTransactionValidatorError::BytecodeSizeTooLarge {
                bytecode_size,
                max_bytecode_size: self.config.max_bytecode_size,
            });
        }

        let contract_class_object_size = serde_json::to_string(&contract_class)
            .expect("Unexpected error serializing contract class.")
            .len();
        if contract_class_object_size > self.config.max_raw_class_size {
            return Err(StatelessTransactionValidatorError::ContractClassObjectSizeTooLarge {
                contract_class_object_size,
                max_contract_class_object_size: self.config.max_raw_class_size,
            });
        }

        Ok(())
    }
}

fn validate_resource_is_non_zero(
    resource_bounds_mapping: &ResourceBoundsMapping,
    resource: Resource,
) -> StatelessTransactionValidatorResult<()> {
    let resource_bounds = match resource {
        Resource::L1Gas => resource_bounds_mapping.l1_gas,
        Resource::L2Gas => resource_bounds_mapping.l2_gas,
    };
    if resource_bounds.max_amount == 0 || resource_bounds.max_price_per_unit == 0 {
        return Err(StatelessTransactionValidatorError::ZeroResourceBounds {
            resource,
            resource_bounds,
        });
    }

    Ok(())
}
