use blockifier::execution::contract_class::ClassInfo;
use blockifier::transaction::account_transaction::AccountTransaction;
use blockifier::transaction::transactions::{
    DeclareTransaction as BlockifierDeclareTransaction,
    DeployAccountTransaction as BlockifierDeployAccountTransaction,
    InvokeTransaction as BlockifierInvokeTransaction,
};
use starknet_api::core::{calculate_contract_address, ChainId, ClassHash, ContractAddress};
use starknet_api::executable_transaction::{
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
    InvokeTransactionV3,
    TransactionHasher,
};
use tracing::error;

use crate::compilation::GatewayCompiler;
use crate::errors::{GatewaySpecError, StatefulTransactionValidatorResult};

/// Converts an RPC transaction to an executable transaction.
/// This conversion is dependent on the chain ID.
/// Note, that for declare transaction this step is heavy, as it requires compilation of Sierra to
/// executable contract class.
pub fn compile_to_casm_and_convert_rpc_to_executable_tx(
    rpc_tx: RpcTransaction,
    gateway_compiler: &GatewayCompiler,
    chain_id: &ChainId,
) -> Result<ExecutableTransaction, GatewaySpecError> {
    Ok(match rpc_tx {
        RpcTransaction::Declare(rpc_declare_tx) => ExecutableTransaction::Declare(
            gateway_compiler.process_declare_tx(rpc_declare_tx, chain_id).map_err(|error| {
                error!(
                    "Failed to convert RPC declare transaction to executable transaction: {}",
                    error
                );
                GatewaySpecError::UnexpectedError { data: "Internal server error".to_owned() }
            })?,
        ),
        RpcTransaction::DeployAccount(rpc_deploy_account_tx) => {
            ExecutableTransaction::DeployAccount(
                ExecutableDeployAccountTransaction::from_rpc_tx(rpc_deploy_account_tx, chain_id)
                    .map_err(|error| {
                        error!(
                            "Failed to convert RPC deploy account transaction to executable \
                             transaction: {}",
                            error
                        );
                        GatewaySpecError::UnexpectedError {
                            data: "Internal server error".to_owned(),
                        }
                    })?,
            )
        }
        RpcTransaction::Invoke(rpc_invoke_tx) => ExecutableTransaction::Invoke(
            ExecutableInvokeTransaction::from_rpc_tx(rpc_invoke_tx, chain_id).map_err(|error| {
                error!(
                    "Failed to convert RPC invoke transaction to executable transaction: {}",
                    error
                );
                GatewaySpecError::UnexpectedError { data: "Internal server error".to_owned() }
            })?,
        ),
    })
}

// TODO(Arni): Remove this function. Replace with a function that take ownership of RpcTransaction.
pub fn rpc_tx_to_account_tx(
    rpc_tx: &RpcTransaction,
    // FIXME(yael 15/4/24): calculate class_info inside the function once compilation code is ready
    optional_class_info: Option<ClassInfo>,
    chain_id: &ChainId,
) -> StatefulTransactionValidatorResult<AccountTransaction> {
    match rpc_tx {
        RpcTransaction::Declare(RpcDeclareTransaction::V3(tx)) => {
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
            let class_info =
                optional_class_info.expect("declare transaction should contain class info");
            let declare_tx = BlockifierDeclareTransaction::new(declare_tx, tx_hash, class_info)
                .map_err(|e| {
                    error!("Failed to convert declare tx hash to blockifier tx type: {}", e);
                    GatewaySpecError::UnexpectedError { data: "Internal server error".to_owned() }
                })?;
            Ok(AccountTransaction::Declare(declare_tx))
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
            let deploy_account_tx = BlockifierDeployAccountTransaction::new(
                deploy_account_tx,
                tx_hash,
                contract_address,
            );
            Ok(AccountTransaction::DeployAccount(deploy_account_tx))
        }
        RpcTransaction::Invoke(RpcInvokeTransaction::V3(tx)) => {
            let invoke_tx = starknet_api::transaction::InvokeTransaction::V3(InvokeTransactionV3 {
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
            let invoke_tx = BlockifierInvokeTransaction::new(invoke_tx, tx_hash);
            Ok(AccountTransaction::Invoke(invoke_tx))
        }
    }
}

// TODO(yael 9/5/54): Should be implemented as part of InternalTransaction in starknet-api
pub fn get_sender_address(tx: &AccountTransaction) -> ContractAddress {
    match tx {
        AccountTransaction::Declare(tx) => match &tx.tx {
            DeclareTransaction::V3(tx) => tx.sender_address,
            _ => panic!("Unsupported transaction version"),
        },
        AccountTransaction::DeployAccount(tx) => tx.contract_address(),
        AccountTransaction::Invoke(tx) => match &tx.tx() {
            starknet_api::transaction::InvokeTransaction::V3(tx) => tx.sender_address,
            _ => panic!("Unsupported transaction version"),
        },
    }
}
