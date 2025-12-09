use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;

use crate::block_hash_manager::input_types::CommitmentTaskInput;
use crate::block_hash_manager::output_types::CommitmentTaskOutput;

pub(crate) mod block_hash_calculator;
pub(crate) mod input_types;
pub(crate) mod output_types;

#[allow(dead_code)]
/// Encapsulates the block hash calculation logic.
pub(crate) struct BlockHashManager {
    pub(crate) tasks_sender: Sender<CommitmentTaskInput>,
    pub(crate) results_receiver: Receiver<CommitmentTaskOutput>,
    pub(crate) commitment_task_performer: JoinHandle<()>,
}
