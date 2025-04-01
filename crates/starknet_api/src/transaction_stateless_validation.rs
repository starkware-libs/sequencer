use crate::data_availability::DataAvailabilityMode;
use crate::rpc_transaction::{
    RpcDeclareTransaction,
    RpcDeployAccountTransaction,
    RpcInvokeTransaction,
    RpcTransaction,
};
use crate::state::SierraContractClass;
use crate::transaction::fields::AllResourceBounds;
use crate::StarknetApiError;

pub trait TransactionStatelessValidation {
    fn validate_contract_address(&self) -> Result<(), StarknetApiError>;
    fn account_deployment_data_is_empty(&self) -> bool;
    fn paymaster_data_is_empty(&self) -> bool;
    fn resource_bounds(&self) -> &AllResourceBounds;
    fn calldata_length(&self) -> Option<usize>;
    fn signature_length(&self) -> usize;
    fn nonce_data_availability_mode(&self) -> &DataAvailabilityMode;
    fn fee_data_availability_mode(&self) -> &DataAvailabilityMode;
    fn is_declare(&self) -> bool;
    fn contract_class(&self) -> Option<&SierraContractClass>;
}

impl TransactionStatelessValidation for RpcTransaction {
    fn validate_contract_address(&self) -> Result<(), StarknetApiError> {
        let sender_address = match &self {
            RpcTransaction::Declare(RpcDeclareTransaction::V3(tx)) => tx.sender_address,
            RpcTransaction::DeployAccount(_) => return Ok(()),
            RpcTransaction::Invoke(RpcInvokeTransaction::V3(tx)) => tx.sender_address,
        };

        sender_address.validate()
    }

    /// The Starknet OS enforces that the deployer data is empty. We add this validation here in the
    /// gateway to prevent transactions from failing the OS.
    fn account_deployment_data_is_empty(&self) -> bool {
        let account_deployment_data = match &self {
            RpcTransaction::DeployAccount(_) => return true, // TO FIX AvivG: not accurate
            RpcTransaction::Declare(RpcDeclareTransaction::V3(tx)) => &tx.account_deployment_data,
            RpcTransaction::Invoke(RpcInvokeTransaction::V3(tx)) => &tx.account_deployment_data,
        };

        account_deployment_data.is_empty()
    }

    fn resource_bounds(&self) -> &AllResourceBounds {
        self.resource_bounds()
    }

    /// The Starknet OS enforces that the paymaster data is empty. We add this validation here in
    /// the gateway to prevent transactions from failing the OS.
    fn paymaster_data_is_empty(&self) -> bool {
        let paymaster_data = match &self {
            RpcTransaction::DeployAccount(RpcDeployAccountTransaction::V3(tx)) => {
                &tx.paymaster_data
            }
            RpcTransaction::Declare(RpcDeclareTransaction::V3(tx)) => &tx.paymaster_data,
            RpcTransaction::Invoke(RpcInvokeTransaction::V3(tx)) => &tx.paymaster_data,
        };

        paymaster_data.is_empty()
    }

    fn calldata_length(&self) -> Option<usize> {
        let calldata = match &self {
            RpcTransaction::Declare(_) => {
                // Declare transaction has no calldata.
                return None;
            }
            RpcTransaction::DeployAccount(RpcDeployAccountTransaction::V3(tx)) => {
                &tx.constructor_calldata
            }
            RpcTransaction::Invoke(RpcInvokeTransaction::V3(tx)) => &tx.calldata,
        };

        Some(calldata.0.len())
    }

    fn signature_length(&self) -> usize {
        let signature = self.signature();

        signature.0.len()
    }

    fn nonce_data_availability_mode(&self) -> &DataAvailabilityMode {
        self.nonce_data_availability_mode()
    }

    fn fee_data_availability_mode(&self) -> &DataAvailabilityMode {
        self.fee_data_availability_mode()
    }

    fn is_declare(&self) -> bool {
        matches!(self, RpcTransaction::Declare(_))
    }

    fn contract_class(&self) -> Option<&SierraContractClass> {
        if let RpcTransaction::Declare(declare_tx) = self {
            let contract_class = match declare_tx {
                RpcDeclareTransaction::V3(tx) => &tx.contract_class,
            };
            Some(contract_class)
        } else {
            None
        }
    }
}
