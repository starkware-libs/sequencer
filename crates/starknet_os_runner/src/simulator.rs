use blockifier::blockifier::transaction_executor::TransactionExecutionOutput;
use blockifier::context::BlockContext;
use blockifier::state::cached_state::StateMaps;
use blockifier_reexecution::execute_single_transaction;
use starknet_api::block::BlockNumber;
use starknet_api::core::ChainId;
use starknet_api::transaction::Transaction;

use crate::errors::SimulationError;

pub struct SimulationOutput {
    pub execution_output: TransactionExecutionOutput,
    pub block_context: BlockContext,
    pub initial_reads: StateMaps,
}

pub trait TxSimulator {
    fn simulate_tx(
        &self,
        block_number: BlockNumber,
        tx: Transaction,
    ) -> Result<SimulationOutput, SimulationError>;
}

pub struct RpcBlockifierTxSimulator {
    pub rpc_url: String,
    pub chain_id: ChainId,
}

impl TxSimulator for RpcBlockifierTxSimulator {
    fn simulate_tx(
        &self,
        block_number: BlockNumber,
        tx: Transaction,
    ) -> Result<SimulationOutput, SimulationError> {
        let (execution_output, initial_reads, block_context) = execute_single_transaction(
            block_number,
            self.rpc_url.clone(),
            self.chain_id.clone(),
            tx,
        )?;

        Ok(SimulationOutput { execution_output, block_context, initial_reads })
    }
}
