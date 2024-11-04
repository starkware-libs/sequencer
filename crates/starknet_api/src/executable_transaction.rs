use serde::{Deserialize, Serialize};

use crate::contract_class::ClassInfo;
use crate::core::{calculate_contract_address, ChainId, ClassHash, ContractAddress, Nonce};
use crate::data_availability::DataAvailabilityMode;
use crate::rpc_transaction::{
    RpcDeployAccountTransaction,
    RpcInvokeTransaction,
    RpcInvokeTransactionV3,
    RpcTransaction,
};
use crate::transaction::{
    AccountDeploymentData,
    AllResourceBounds,
    Calldata,
    ContractAddressSalt,
    Fee,
    PaymasterData,
    Tip,
    TransactionHash,
    TransactionHasher,
    TransactionSignature,
    TransactionVersion,
    ValidResourceBounds,
};
use crate::StarknetApiError;

macro_rules! implement_inner_tx_getter_calls {
    ($(($field:ident, $field_type:ty)),*) => {
        $(pub fn $field(&self) -> $field_type {
            self.tx.$field().clone()
        })*
    };
}

macro_rules! implement_getter_calls {
    ($(($field:ident, $field_type:ty)),*) => {
        $(pub fn $field(&self) -> $field_type {
            self.$field
        })*
};
}

// TODO: Remove after introducing new transaction type.
pub type Transaction = AccountTransaction;

/// Represents a paid Starknet transaction.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AccountTransaction {
    Declare(DeclareTransaction),
    DeployAccount(DeployAccountTransaction),
    Invoke(InvokeTransaction),
}

impl AccountTransaction {
    pub fn contract_address(&self) -> ContractAddress {
        match self {
            AccountTransaction::Declare(tx_data) => tx_data.tx.sender_address(),
            AccountTransaction::DeployAccount(tx_data) => tx_data.contract_address,
            AccountTransaction::Invoke(tx_data) => tx_data.tx.sender_address(),
        }
    }

    pub fn sender_address(&self) -> ContractAddress {
        self.contract_address()
    }

    pub fn nonce(&self) -> Nonce {
        match self {
            AccountTransaction::Declare(tx_data) => tx_data.tx.nonce(),
            AccountTransaction::DeployAccount(tx_data) => tx_data.tx.nonce(),
            AccountTransaction::Invoke(tx_data) => tx_data.tx.nonce(),
        }
    }

    pub fn tx_hash(&self) -> TransactionHash {
        match self {
            AccountTransaction::Declare(tx_data) => tx_data.tx_hash,
            AccountTransaction::DeployAccount(tx_data) => tx_data.tx_hash,
            AccountTransaction::Invoke(tx_data) => tx_data.tx_hash,
        }
    }

    // TODO(Mohammad): add a getter macro.
    pub fn tip(&self) -> Option<Tip> {
        match self {
            AccountTransaction::Declare(declare_tx) => match &declare_tx.tx {
                crate::transaction::DeclareTransaction::V3(tx_v3) => Some(tx_v3.tip),
                _ => None,
            },
            AccountTransaction::DeployAccount(deploy_account_tx) => match &deploy_account_tx.tx {
                crate::transaction::DeployAccountTransaction::V3(tx_v3) => Some(tx_v3.tip),
                _ => None,
            },
            AccountTransaction::Invoke(invoke_tx) => match &invoke_tx.tx {
                crate::transaction::InvokeTransaction::V3(tx_v3) => Some(tx_v3.tip),
                _ => None,
            },
        }
    }

    pub fn resource_bounds(&self) -> Option<&ValidResourceBounds> {
        match self {
            AccountTransaction::Declare(declare_tx) => match &declare_tx.tx {
                crate::transaction::DeclareTransaction::V3(tx_v3) => Some(&tx_v3.resource_bounds),
                _ => None,
            },
            AccountTransaction::DeployAccount(deploy_account_tx) => match &deploy_account_tx.tx {
                crate::transaction::DeployAccountTransaction::V3(tx_v3) => {
                    Some(&tx_v3.resource_bounds)
                }
                _ => None,
            },
            AccountTransaction::Invoke(invoke_tx) => match &invoke_tx.tx {
                crate::transaction::InvokeTransaction::V3(tx_v3) => Some(&tx_v3.resource_bounds),
                _ => None,
            },
        }
    }

    // TODO(Arni): Update the function to support all transaction types.
    pub fn new_from_rpc_tx(
        rpc_tx: RpcTransaction,
        tx_hash: TransactionHash,
        sender_address: ContractAddress,
    ) -> AccountTransaction {
        AccountTransaction::Invoke(crate::executable_transaction::InvokeTransaction {
            tx: crate::transaction::InvokeTransaction::V3(
                crate::transaction::InvokeTransactionV3 {
                    sender_address,
                    tip: *rpc_tx.tip(),
                    nonce: *rpc_tx.nonce(),
                    resource_bounds: ValidResourceBounds::AllResources(*rpc_tx.resource_bounds()),
                    signature: TransactionSignature::default(),
                    calldata: Calldata::default(),
                    nonce_data_availability_mode: DataAvailabilityMode::L1,
                    fee_data_availability_mode: DataAvailabilityMode::L1,
                    paymaster_data: PaymasterData::default(),
                    account_deployment_data: AccountDeploymentData::default(),
                },
            ),
            tx_hash,
        })
    }
}

// TODO: replace with proper implementation.
impl From<AccountTransaction> for RpcTransaction {
    fn from(tx: AccountTransaction) -> Self {
        Self::Invoke(RpcInvokeTransaction::V3(RpcInvokeTransactionV3 {
            sender_address: tx.contract_address(),
            tip: tx.tip().unwrap_or_default(),
            nonce: Nonce::default(),
            resource_bounds: match tx.resource_bounds() {
                Some(ValidResourceBounds::AllResources(all_resource_bounds)) => {
                    *all_resource_bounds
                }
                _ => AllResourceBounds::default(),
            },
            signature: TransactionSignature::default(),
            calldata: Calldata::default(),
            nonce_data_availability_mode: DataAvailabilityMode::L1,
            fee_data_availability_mode: DataAvailabilityMode::L1,
            paymaster_data: PaymasterData::default(),
            account_deployment_data: AccountDeploymentData::default(),
        }))
    }
}

// TODO(Mohammad): Add constructor for all the transaction's structs.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DeclareTransaction {
    pub tx: crate::transaction::DeclareTransaction,
    pub tx_hash: TransactionHash,
    pub class_info: ClassInfo,
}

impl DeclareTransaction {
    pub fn create(
        declare_tx: crate::transaction::DeclareTransaction,
        class_info: ClassInfo,
        chain_id: &ChainId,
    ) -> Result<Self, StarknetApiError> {
        let tx_hash = declare_tx.calculate_transaction_hash(chain_id, &declare_tx.version())?;
        Ok(Self { tx: declare_tx, tx_hash, class_info })
    }

    /// Validates that the compiled class hash of the compiled contract class matches the supplied
    /// compiled class hash.
    /// Relevant only for version 3 transactions.
    pub fn validate_compiled_class_hash(&self) -> bool {
        let supplied_compiled_class_hash = match &self.tx {
            crate::transaction::DeclareTransaction::V3(tx) => tx.compiled_class_hash,
            crate::transaction::DeclareTransaction::V2(tx) => tx.compiled_class_hash,
            crate::transaction::DeclareTransaction::V1(_)
            | crate::transaction::DeclareTransaction::V0(_) => return true,
        };

        let contract_class = &self.class_info.contract_class;
        let compiled_class_hash = contract_class.compiled_class_hash();

        compiled_class_hash == supplied_compiled_class_hash
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DeployAccountTransaction {
    pub tx: crate::transaction::DeployAccountTransaction,
    pub tx_hash: TransactionHash,
    pub contract_address: ContractAddress,
}

impl DeployAccountTransaction {
    implement_inner_tx_getter_calls!(
        (class_hash, ClassHash),
        (constructor_calldata, Calldata),
        (contract_address_salt, ContractAddressSalt),
        (nonce, Nonce),
        (signature, TransactionSignature),
        (version, TransactionVersion),
        (resource_bounds, ValidResourceBounds),
        (tip, Tip),
        (nonce_data_availability_mode, DataAvailabilityMode),
        (fee_data_availability_mode, DataAvailabilityMode),
        (paymaster_data, PaymasterData)
    );
    implement_getter_calls!((tx_hash, TransactionHash), (contract_address, ContractAddress));

    pub fn tx(&self) -> &crate::transaction::DeployAccountTransaction {
        &self.tx
    }

    pub fn create(
        deploy_account_tx: crate::transaction::DeployAccountTransaction,
        chain_id: &ChainId,
    ) -> Result<Self, StarknetApiError> {
        let contract_address = calculate_contract_address(
            deploy_account_tx.contract_address_salt(),
            deploy_account_tx.class_hash(),
            &deploy_account_tx.constructor_calldata(),
            ContractAddress::default(),
        )?;
        let tx_hash =
            deploy_account_tx.calculate_transaction_hash(chain_id, &deploy_account_tx.version())?;
        Ok(Self { tx: deploy_account_tx, tx_hash, contract_address })
    }

    pub fn from_rpc_tx(
        rpc_tx: RpcDeployAccountTransaction,
        chain_id: &ChainId,
    ) -> Result<Self, StarknetApiError> {
        let deploy_account_tx: crate::transaction::DeployAccountTransaction = rpc_tx.into();
        Self::create(deploy_account_tx, chain_id)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InvokeTransaction {
    pub tx: crate::transaction::InvokeTransaction,
    pub tx_hash: TransactionHash,
}

impl InvokeTransaction {
    implement_inner_tx_getter_calls!(
        (calldata, Calldata),
        (nonce, Nonce),
        (signature, TransactionSignature),
        (sender_address, ContractAddress),
        (version, TransactionVersion),
        (resource_bounds, ValidResourceBounds),
        (tip, Tip),
        (nonce_data_availability_mode, DataAvailabilityMode),
        (fee_data_availability_mode, DataAvailabilityMode),
        (paymaster_data, PaymasterData),
        (account_deployment_data, AccountDeploymentData)
    );
    implement_getter_calls!((tx_hash, TransactionHash));

    pub fn tx(&self) -> &crate::transaction::InvokeTransaction {
        &self.tx
    }

    pub fn create(
        invoke_tx: crate::transaction::InvokeTransaction,
        chain_id: &ChainId,
    ) -> Result<Self, StarknetApiError> {
        let tx_hash = invoke_tx.calculate_transaction_hash(chain_id, &invoke_tx.version())?;
        Ok(Self { tx: invoke_tx, tx_hash })
    }

    pub fn from_rpc_tx(
        rpc_tx: RpcInvokeTransaction,
        chain_id: &ChainId,
    ) -> Result<Self, StarknetApiError> {
        let invoke_tx: crate::transaction::InvokeTransaction = rpc_tx.into();
        Self::create(invoke_tx, chain_id)
    }
}

#[derive(Clone, Debug)]
pub struct L1HandlerTransaction {
    pub tx: crate::transaction::L1HandlerTransaction,
    pub tx_hash: TransactionHash,
    pub paid_fee_on_l1: Fee,
}

impl L1HandlerTransaction {
    pub fn payload_size(&self) -> usize {
        // The calldata includes the "from" field, which is not a part of the payload.
        self.tx.calldata.0.len() - 1
    }
}
