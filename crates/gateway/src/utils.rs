use starknet_api::core::ChainId;
use starknet_api::executable_transaction::{
    DeployAccountTransaction as ExecutableDeployAccountTransaction,
    InvokeTransaction as ExecutableInvokeTransaction,
    Transaction as ExecutableTransaction,
};
use starknet_api::rpc_transaction::RpcTransaction;
use tracing::error;

use crate::compilation::GatewayCompiler;
use crate::errors::GatewaySpecError;

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
