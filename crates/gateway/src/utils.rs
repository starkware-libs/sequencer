use starknet_api::core::ChainId;
use starknet_api::executable_transaction::{
    AccountTransaction as ExecutableTransaction,
    DeclareTransaction as ExecutableDeclareTransaction,
    DeployAccountTransaction as ExecutableDeployAccountTransaction,
    InvokeTransaction as ExecutableInvokeTransaction,
};
use starknet_api::rpc_transaction::{RpcDeclareTransaction, RpcTransaction};
use starknet_gateway_types::errors::GatewaySpecError;
use tracing::{debug, error};

use crate::compilation::GatewayCompiler;
use crate::errors::GatewayResult;

/// Converts an RPC transaction to an executable transaction.
/// Note, for declare transaction this step is heavy, as it requires compilation of Sierra to
/// executable contract class.
pub fn compile_contract_and_build_executable_tx(
    rpc_tx: RpcTransaction,
    gateway_compiler: &GatewayCompiler,
    chain_id: &ChainId,
) -> GatewayResult<ExecutableTransaction> {
    Ok(match rpc_tx {
        RpcTransaction::Declare(rpc_declare_tx) => {
            let executable_declare_tx = compile_contract_and_build_executable_declare_tx(
                rpc_declare_tx,
                gateway_compiler,
                chain_id,
            )?;
            ExecutableTransaction::Declare(executable_declare_tx)
        }
        RpcTransaction::DeployAccount(rpc_deploy_account_tx) => {
            let executable_deploy_account_tx =
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
                    })?;
            ExecutableTransaction::DeployAccount(executable_deploy_account_tx)
        }
        RpcTransaction::Invoke(rpc_invoke_tx) => {
            let executable_invoke_tx = ExecutableInvokeTransaction::from_rpc_tx(
                rpc_invoke_tx,
                chain_id,
            )
            .map_err(|error| {
                error!(
                    "Failed to convert RPC invoke transaction to executable transaction: {}",
                    error
                );
                GatewaySpecError::UnexpectedError { data: "Internal server error".to_owned() }
            })?;
            ExecutableTransaction::Invoke(executable_invoke_tx)
        }
    })
}

fn compile_contract_and_build_executable_declare_tx(
    rpc_tx: RpcDeclareTransaction,
    gateway_compiler: &GatewayCompiler,
    chain_id: &ChainId,
) -> GatewayResult<ExecutableDeclareTransaction> {
    let class_info = gateway_compiler.process_declare_tx(&rpc_tx)?;
    let declare_tx: starknet_api::transaction::DeclareTransaction = rpc_tx.into();
    let executable_declare_tx =
        ExecutableDeclareTransaction::create(declare_tx, class_info, chain_id).map_err(|err| {
            debug!("Failed to create executable declare transaction {:?}", err);
            GatewaySpecError::UnexpectedError { data: "Internal server error.".to_owned() }
        })?;

    Ok(executable_declare_tx)
}
