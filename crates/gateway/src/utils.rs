use blockifier::execution::contract_class::ClassInfo;
use blockifier::transaction::account_transaction::AccountTransaction;
use blockifier::transaction::transactions::{
    DeclareTransaction as BlockifierDeclareTransaction,
    DeployAccountTransaction as BlockifierDeployAccountTransaction,
    InvokeTransaction as BlockifierInvokeTransaction,
};
use starknet_api::core::{calculate_contract_address, ChainId, ClassHash, ContractAddress, Nonce};
use starknet_api::rpc_transaction::{
    RPCDeclareTransaction, RPCDeployAccountTransaction, RPCInvokeTransaction, RPCTransaction,
};
use starknet_api::transaction::{
    DeclareTransaction, DeclareTransactionV3, DeployAccountTransaction, DeployAccountTransactionV3,
    InvokeTransaction, InvokeTransactionV3, Tip, TransactionHash, TransactionHasher,
};
use starknet_mempool_types::mempool_types::ThinTransaction;

use crate::errors::StatefulTransactionValidatorResult;

#[cfg(test)]
#[path = "utils_test.rs"]
mod utils_test;

macro_rules! implement_ref_getters {
    ($(($member_name:ident, $member_type:ty));* $(;)?) => {
        $(fn $member_name(&self) -> &$member_type {
            match self {
                starknet_api::rpc_transaction::RPCTransaction::Declare(
                    starknet_api::rpc_transaction::RPCDeclareTransaction::V3(tx)
                ) => &tx.$member_name,
                starknet_api::rpc_transaction::RPCTransaction::DeployAccount(
                    starknet_api::rpc_transaction::RPCDeployAccountTransaction::V3(tx)
                ) => &tx.$member_name,
                starknet_api::rpc_transaction::RPCTransaction::Invoke(
                    starknet_api::rpc_transaction::RPCInvokeTransaction::V3(tx)
                ) => &tx.$member_name,
            }
        })*
    };
}

impl RPCTransactionExt for RPCTransaction {
    implement_ref_getters!(
        (nonce, Nonce);
        (tip, Tip)
    );
}

pub fn external_tx_to_thin_tx(
    external_tx: &RPCTransaction,
    tx_hash: TransactionHash,
) -> ThinTransaction {
    ThinTransaction {
        tip: *external_tx.tip(),
        nonce: *external_tx.nonce(),
        sender_address: get_sender_address(external_tx),
        tx_hash,
    }
}

pub fn get_sender_address(tx: &RPCTransaction) -> ContractAddress {
    match tx {
        RPCTransaction::Declare(RPCDeclareTransaction::V3(tx)) => tx.sender_address,
        // TODO(Mohammad): Add support for deploy account.
        RPCTransaction::DeployAccount(RPCDeployAccountTransaction::V3(_)) => {
            ContractAddress::default()
        }
        RPCTransaction::Invoke(RPCInvokeTransaction::V3(tx)) => tx.sender_address,
    }
}

// TODO(Mohammad): Remove this trait once it is implemented in StarkNet API.
#[allow(dead_code)]
pub trait RPCTransactionExt {
    fn nonce(&self) -> &Nonce;
    fn tip(&self) -> &Tip;
}

pub fn external_tx_to_account_tx(
    external_tx: &RPCTransaction,
    // FIXME(yael 15/4/24): calculate class_info inside the function once compilation code is ready
    optional_class_info: Option<ClassInfo>,
    chain_id: &ChainId,
) -> StatefulTransactionValidatorResult<AccountTransaction> {
    match external_tx {
        RPCTransaction::Declare(RPCDeclareTransaction::V3(tx)) => {
            let declare_tx = DeclareTransaction::V3(DeclareTransactionV3 {
                class_hash: ClassHash::default(), /* FIXME(yael 15/4/24): call the starknet-api
                                                   * function once ready */
                resource_bounds: tx.resource_bounds.clone().into(),
                tip: tx.tip,
                signature: tx.signature.clone(),
                nonce: tx.nonce,
                compiled_class_hash: tx.compiled_class_hash,
                sender_address: tx.sender_address,
                nonce_data_availability_mode: tx.nonce_data_availability_mode,
                fee_data_availability_mode: tx.fee_data_availability_mode,
                paymaster_data: tx.paymaster_data.clone(),
                account_deployment_data: tx.account_deployment_data.clone(),
            });
            let tx_hash = declare_tx.calculate_transaction_hash(chain_id, &declare_tx.version())?;
            let class_info =
                optional_class_info.expect("declare transaction should contain class info");
            let declare_tx = BlockifierDeclareTransaction::new(declare_tx, tx_hash, class_info)?;
            Ok(AccountTransaction::Declare(declare_tx))
        }
        RPCTransaction::DeployAccount(RPCDeployAccountTransaction::V3(tx)) => {
            let deploy_account_tx = DeployAccountTransaction::V3(DeployAccountTransactionV3 {
                resource_bounds: tx.resource_bounds.clone().into(),
                tip: tx.tip,
                signature: tx.signature.clone(),
                nonce: tx.nonce,
                class_hash: tx.class_hash,
                contract_address_salt: tx.contract_address_salt,
                constructor_calldata: tx.constructor_calldata.clone(),
                nonce_data_availability_mode: tx.nonce_data_availability_mode,
                fee_data_availability_mode: tx.fee_data_availability_mode,
                paymaster_data: tx.paymaster_data.clone(),
            });
            let contract_address = calculate_contract_address(
                deploy_account_tx.contract_address_salt(),
                deploy_account_tx.class_hash(),
                &deploy_account_tx.constructor_calldata(),
                ContractAddress::default(),
            )?;
            let tx_hash = deploy_account_tx
                .calculate_transaction_hash(chain_id, &deploy_account_tx.version())?;
            let deploy_account_tx = BlockifierDeployAccountTransaction::new(
                deploy_account_tx,
                tx_hash,
                contract_address,
            );
            Ok(AccountTransaction::DeployAccount(deploy_account_tx))
        }
        RPCTransaction::Invoke(RPCInvokeTransaction::V3(tx)) => {
            let invoke_tx = InvokeTransaction::V3(InvokeTransactionV3 {
                resource_bounds: tx.resource_bounds.clone().into(),
                tip: tx.tip,
                signature: tx.signature.clone(),
                nonce: tx.nonce,
                sender_address: tx.sender_address,
                calldata: tx.calldata.clone(),
                nonce_data_availability_mode: tx.nonce_data_availability_mode,
                fee_data_availability_mode: tx.fee_data_availability_mode,
                paymaster_data: tx.paymaster_data.clone(),
                account_deployment_data: tx.account_deployment_data.clone(),
            });
            let tx_hash = invoke_tx.calculate_transaction_hash(chain_id, &invoke_tx.version())?;
            let invoke_tx = BlockifierInvokeTransaction::new(invoke_tx, tx_hash);
            Ok(AccountTransaction::Invoke(invoke_tx))
        }
    }
}

// TODO(yael 9/5/54): Remove once we we transition to InternalTransaction
pub fn get_tx_hash(tx: &AccountTransaction) -> TransactionHash {
    match tx {
        AccountTransaction::Declare(tx) => tx.tx_hash,
        AccountTransaction::DeployAccount(tx) => tx.tx_hash,
        AccountTransaction::Invoke(tx) => tx.tx_hash,
    }
}

/// Checks whether 'subsequence' is a subsequence of 'sequence'.
pub fn is_subsequence<T: Eq>(subsequence: &[T], sequence: &[T]) -> bool {
    let mut offset = 0;

    for item in sequence {
        if offset == subsequence.len() {
            return true;
        }

        if item == &subsequence[offset] {
            offset += 1;
        }
    }

    offset == subsequence.len()
}
