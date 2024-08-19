use blockifier::transaction::account_transaction::AccountTransaction;
use blockifier::transaction::transactions::{
    DeclareTransaction as BlockifierDeclareTransaction,
    DeployAccountTransaction as BlockifierDeployAccountTransaction,
    InvokeTransaction as BlockifierInvokeTransaction,
};
use starknet_api::core::{calculate_contract_address, ChainId, ClassHash, ContractAddress};
use starknet_api::executable_transaction::{
    DeclareTransaction as ExecutableDeclareTransaction,
    DeployAccountTransaction as ExecutableDeployAccountTransaction,
    InvokeTransaction as ExecutableInvokeTransaction,
    Transaction as ExecutableTransaction,
};
use starknet_api::rpc_transaction::{
    RpcDeclareTransaction,
    RpcDeployAccountTransaction,
    RpcInvokeTransaction,
    RpcTransaction,
};
use starknet_api::transaction::{
    DeclareTransaction,
    DeclareTransactionV3,
    DeployAccountTransaction,
    DeployAccountTransactionV3,
    InvokeTransaction,
    InvokeTransactionV3,
    TransactionHash,
    TransactionHasher,
};
use starknet_mempool_types::mempool_types::ThinTransaction;
use tracing::error;

use crate::compilation::GatewayCompiler;
use crate::errors::GatewaySpecError;

pub fn external_tx_to_thin_tx(
    external_tx: &RpcTransaction,
    tx_hash: TransactionHash,
    sender_address: ContractAddress,
) -> ThinTransaction {
    ThinTransaction {
        tip: *external_tx.tip(),
        nonce: *external_tx.nonce(),
        sender_address,
        tx_hash,
    }
}

pub fn external_tx_to_executable_tx(
    external_tx: &RpcTransaction,
    gateway_compiler: &GatewayCompiler,
    chain_id: &ChainId,
) -> Result<ExecutableTransaction, GatewaySpecError> {
    Ok(match external_tx {
        RpcTransaction::Declare(rpc_declare_tx) => {
            let class_info = gateway_compiler.process_declare_tx(rpc_declare_tx)?;
            let RpcDeclareTransaction::V3(tx) = rpc_declare_tx;
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
            let tx_hash = declare_tx
                .calculate_transaction_hash(chain_id, &declare_tx.version())
                .map_err(|e| {
                    error!("Failed to calculate tx hash: {}", e);
                    GatewaySpecError::UnexpectedError { data: "Internal server error".to_owned() }
                })?;
            let declare_tx = ExecutableDeclareTransaction { tx: declare_tx, tx_hash, class_info };
            ExecutableTransaction::Declare(declare_tx)
        }
        RpcTransaction::DeployAccount(RpcDeployAccountTransaction::V3(tx)) => {
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
            )
            .map_err(|e| {
                error!("Failed to calculate contract address: {}", e);
                GatewaySpecError::UnexpectedError { data: "Internal server error".to_owned() }
            })?;
            let tx_hash = deploy_account_tx
                .calculate_transaction_hash(chain_id, &deploy_account_tx.version())
                .map_err(|e| {
                    error!("Failed to calculate tx hash: {}", e);
                    GatewaySpecError::UnexpectedError { data: "Internal server error".to_owned() }
                })?;
            ExecutableTransaction::DeployAccount(ExecutableDeployAccountTransaction {
                tx: deploy_account_tx,
                tx_hash,
                contract_address,
            })
        }
        RpcTransaction::Invoke(RpcInvokeTransaction::V3(tx)) => {
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
            let tx_hash = invoke_tx
                .calculate_transaction_hash(chain_id, &invoke_tx.version())
                .map_err(|e| {
                    error!("Failed to calculate tx hash: {}", e);
                    GatewaySpecError::UnexpectedError { data: "Internal server error".to_owned() }
                })?;
            ExecutableTransaction::Invoke(ExecutableInvokeTransaction { tx: invoke_tx, tx_hash })
        }
    })
}

// TODO(Arni): Move this function into the blockifier - use a try into. This is exactly the code
// dedup we are working on.
pub fn executable_transaction_to_account_tx(
    tx: &ExecutableTransaction,
) -> Result<AccountTransaction, GatewaySpecError> {
    match tx {
        ExecutableTransaction::Declare(declare_tx) => {
            let declare_tx =
                BlockifierDeclareTransaction::try_from(declare_tx.clone()).map_err(|error| {
                    error!("Failed to convert declare tx: {}", error);
                    GatewaySpecError::UnexpectedError { data: "Internal server error".to_owned() }
                })?;
            Ok(AccountTransaction::Declare(declare_tx))
        }
        ExecutableTransaction::DeployAccount(ExecutableDeployAccountTransaction {
            tx: deploy_account_tx,
            tx_hash,
            contract_address,
        }) => {
            let deploy_account_tx = BlockifierDeployAccountTransaction::new(
                deploy_account_tx.clone(),
                *tx_hash,
                *contract_address,
            );
            Ok(AccountTransaction::DeployAccount(deploy_account_tx))
        }
        ExecutableTransaction::Invoke(ExecutableInvokeTransaction { tx: invoke_tx, tx_hash }) => {
            let invoke_tx = BlockifierInvokeTransaction::new(invoke_tx.clone(), *tx_hash);
            Ok(AccountTransaction::Invoke(invoke_tx))
        }
    }
}
