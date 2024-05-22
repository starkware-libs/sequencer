use blockifier::execution::contract_class::ClassInfo;
use blockifier::transaction::account_transaction::AccountTransaction;
use blockifier::transaction::transactions::{
    DeclareTransaction as BlockifierDeclareTransaction,
    DeployAccountTransaction as BlockifierDeployAccountTransaction,
    InvokeTransaction as BlockifierInvokeTransaction,
};
use starknet_api::core::{calculate_contract_address, ChainId, ClassHash, ContractAddress, Nonce};
use starknet_api::external_transaction::{
    ExternalDeclareTransaction, ExternalDeployAccountTransaction, ExternalInvokeTransaction,
    ExternalTransaction,
};
use starknet_api::transaction::{
    DeclareTransaction, DeclareTransactionV3, DeployAccountTransaction, DeployAccountTransactionV3,
    InvokeTransaction, InvokeTransactionV3, Tip, TransactionHash, TransactionHasher,
};
use starknet_mempool_types::mempool_types::ThinTransaction;

use crate::errors::StatefulTransactionValidatorResult;
use crate::starknet_api_test_utils::get_sender_address;

macro_rules! implement_ref_getters {
    ($(($member_name:ident, $member_type:ty));* $(;)?) => {
        $(fn $member_name(&self) -> &$member_type {
            match self {
                starknet_api::external_transaction::ExternalTransaction::Declare(
                    starknet_api::external_transaction::ExternalDeclareTransaction::V3(tx)
                ) => &tx.$member_name,
                starknet_api::external_transaction::ExternalTransaction::DeployAccount(
                    starknet_api::external_transaction::ExternalDeployAccountTransaction::V3(tx)
                ) => &tx.$member_name,
                starknet_api::external_transaction::ExternalTransaction::Invoke(
                    starknet_api::external_transaction::ExternalInvokeTransaction::V3(tx)
                ) => &tx.$member_name,
            }
        })*
    };
}

impl ExternalTransactionExt for ExternalTransaction {
    implement_ref_getters!(
        (nonce, Nonce);
        (tip, Tip)
    );
}

pub fn external_tx_to_thin_tx(external_tx: &ExternalTransaction) -> ThinTransaction {
    ThinTransaction {
        tip: *external_tx.tip(),
        nonce: *external_tx.nonce(),
        contract_address: get_sender_address(external_tx),
        // TODO(Yael): Add transaction hash calculation.
        tx_hash: TransactionHash::default(),
    }
}

// TODO(Mohammad): Remove this trait once it is implemented in StarkNet API.
pub trait ExternalTransactionExt {
    fn nonce(&self) -> &Nonce;
    fn tip(&self) -> &Tip;
}

pub fn external_tx_to_account_tx(
    external_tx: &ExternalTransaction,
    // FIXME(yael 15/4/24): calculate class_info inside the function once compilation code is ready
    optional_class_info: Option<ClassInfo>,
    chain_id: &ChainId,
) -> StatefulTransactionValidatorResult<AccountTransaction> {
    match external_tx {
        ExternalTransaction::Declare(ExternalDeclareTransaction::V3(tx)) => {
            let declare_tx = DeclareTransaction::V3(DeclareTransactionV3 {
                class_hash: ClassHash::default(), /* FIXME(yael 15/4/24): call the starknet-api
                                                   * function once ready */
                resource_bounds: tx.resource_bounds.clone(),
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
        ExternalTransaction::DeployAccount(ExternalDeployAccountTransaction::V3(tx)) => {
            let deploy_account_tx = DeployAccountTransaction::V3(DeployAccountTransactionV3 {
                resource_bounds: tx.resource_bounds.clone(),
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
        ExternalTransaction::Invoke(ExternalInvokeTransaction::V3(tx)) => {
            let invoke_tx = InvokeTransaction::V3(InvokeTransactionV3 {
                resource_bounds: tx.resource_bounds.clone(),
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
