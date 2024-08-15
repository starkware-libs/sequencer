use blockifier::transaction::account_transaction::AccountTransaction;
use starknet_api::core::{ChainId, ContractAddress};
use starknet_api::executable_transaction::Transaction as ExecutableTransaction;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::DeclareTransaction;
use tracing::error;

use crate::compilation::GatewayCompiler;
use crate::errors::GatewaySpecError;

pub fn rpc_tx_to_executable_tx(
    rpc_tx: &RpcTransaction,
    gateway_compiler: &GatewayCompiler,
    chain_id: &ChainId,
) -> Result<ExecutableTransaction, GatewaySpecError> {
    let class_info = if let RpcTransaction::Declare(rpc_declare_tx) = rpc_tx {
        Some(gateway_compiler.process_declare_tx(rpc_declare_tx)?)
    } else {
        None
    };

    ExecutableTransaction::from_rpc_tx(rpc_tx, class_info, chain_id).map_err(|error| {
        error!("Failed to convert RPC transaction to executable transaction: {}", error);
        GatewaySpecError::UnexpectedError { data: "Internal server error".to_owned() }
    })
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
