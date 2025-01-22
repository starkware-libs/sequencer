use starknet_api::contract_class::ClassInfo;
use starknet_api::core::ChainId;
use starknet_api::executable_transaction::{
    AccountTransaction as ExecutableTransaction,
    DeclareTransaction as ExecutableDeclareTransaction,
    DeployAccountTransaction as ExecutableDeployAccountTransaction,
    InvokeTransaction as ExecutableInvokeTransaction,
};
use starknet_api::rpc_transaction::{
    InternalRpcDeclareTransactionV3,
    InternalRpcDeployAccountTransaction,
    InternalRpcTransactionWithoutTxHash,
    RpcDeclareTransaction,
    RpcDeployAccountTransaction,
    RpcTransaction,
};
use starknet_api::transaction::CalculateContractAddress;
use starknet_gateway_types::errors::GatewaySpecError;
use tracing::{debug, error};

use crate::compilation::GatewayCompiler;
use crate::errors::GatewayResult;

// TODO(Arni): Share code with [convert_rpc_tx_to_internal_rpc_tx]. Probably delete this function
// and use that function instead.
/// Converts an RPC transaction to an internal rpc transaction.
/// Note, for declare transaction this step is heavy, as it requires compilation of Sierra to
/// executable contract class.
pub fn compile_contract_and_build_internal_rpc_tx(
    rpc_tx: RpcTransaction,
    gateway_compiler: &GatewayCompiler,
) -> GatewayResult<(Option<ClassInfo>, InternalRpcTransactionWithoutTxHash)> {
    Ok(match rpc_tx {
        RpcTransaction::Declare(tx) => {
            let (class_info, tx) =
                compile_contract_and_build_internal_rpc_declare_tx(tx, gateway_compiler)?;

            (Some(class_info), InternalRpcTransactionWithoutTxHash::Declare(tx))
        }
        RpcTransaction::DeployAccount(RpcDeployAccountTransaction::V3(tx)) => {
            let contract_address = tx.calculate_contract_address().map_err(|error| {
                error!(
                    "Failed to convert RPC deploy account transaction to executable transaction: \
                     {}",
                    error
                );
                GatewaySpecError::UnexpectedError { data: "Internal server error".to_owned() }
            })?;
            (
                None,
                InternalRpcTransactionWithoutTxHash::DeployAccount(
                    InternalRpcDeployAccountTransaction {
                        tx: RpcDeployAccountTransaction::V3(tx),
                        contract_address,
                    },
                ),
            )
        }
        RpcTransaction::Invoke(tx) => (None, InternalRpcTransactionWithoutTxHash::Invoke(tx)),
    })
}

// TODO(Arni): Delete this function.
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

/// Returns the compiled class's class info and an internal RPC declare transaction.
fn compile_contract_and_build_internal_rpc_declare_tx(
    rpc_tx: RpcDeclareTransaction,
    gateway_compiler: &GatewayCompiler,
) -> GatewayResult<(ClassInfo, InternalRpcDeclareTransactionV3)> {
    // TODO(Arni): Use the class manager to create the executable tx.
    let class_info = gateway_compiler.process_declare_tx(&rpc_tx)?;
    let internal_rpc_tx = match rpc_tx {
        RpcDeclareTransaction::V3(tx) => InternalRpcDeclareTransactionV3 {
            sender_address: tx.sender_address,
            compiled_class_hash: tx.compiled_class_hash,
            signature: tx.signature,
            nonce: tx.nonce,
            class_hash: tx.contract_class.calculate_class_hash(),
            resource_bounds: tx.resource_bounds,
            tip: tx.tip,
            paymaster_data: tx.paymaster_data,
            account_deployment_data: tx.account_deployment_data,
            nonce_data_availability_mode: tx.nonce_data_availability_mode,
            fee_data_availability_mode: tx.fee_data_availability_mode,
        },
    };

    Ok((class_info, internal_rpc_tx))
}

fn compile_contract_and_build_executable_declare_tx(
    rpc_tx: RpcDeclareTransaction,
    gateway_compiler: &GatewayCompiler,
    chain_id: &ChainId,
) -> GatewayResult<ExecutableDeclareTransaction> {
    let class_info = gateway_compiler.process_declare_tx(&rpc_tx)?;
    // TODO(Arni): Convert to internal tx and use the class manager to create the executable tx.
    let declare_tx: starknet_api::transaction::DeclareTransaction = rpc_tx.into();
    let executable_declare_tx =
        ExecutableDeclareTransaction::create(declare_tx, class_info, chain_id).map_err(|err| {
            debug!("Failed to create executable declare transaction {:?}", err);
            GatewaySpecError::UnexpectedError { data: "Internal server error.".to_owned() }
        })?;

    Ok(executable_declare_tx)
}
