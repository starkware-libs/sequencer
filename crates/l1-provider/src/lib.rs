pub mod errors;

use starknet_api::transaction::{L1HandlerTransaction, TransactionHash};

use crate::errors::L1ProviderError;

type L1ProviderResult<T> = Result<T, L1ProviderError>;

// TODO: optimistic proposer support, will add later to keep things simple, but the design here
// is compatible with it.
pub struct L1Provider;

impl L1Provider {
    pub async fn new(_config: L1ProviderConfig) -> L1ProviderResult<Self> {
        todo!(
            "init crawler to start next crawl from ~1 hour ago, this can have l1 errors when \
             finding the latest block on L1 to 'subtract' 1 hour from."
        );
    }

    pub fn get_txs(&mut self) -> L1ProviderResult<&[L1HandlerTransaction]> {
        todo!(
            "Stage and return references to unconsumed L1 messages that are not in L2 from \
             internal buffer, to be serialized and sent to batcher. Error if not in Propose state."
        );
    }

    pub fn validate(&self, _tx: &L1HandlerTransaction) -> L1ProviderResult<bool> {
        todo!(
            "Check that tx is unconsumed and not present in L2. Error if in Propose state, NOP if \
             in pending state (likely due to a crash and losing one validator for the block's \
             duration node isn't serious)."
        )
    }

    // TODO: when deciding on consensus, if possible, have commit_block also tell the node if it's
    // about to [optimistically-]propose or validate the next block.
    pub fn commit_block(&mut self, _commited_txs: &[TransactionHash]) {
        todo!(
            "Purges txs from internal buffers, if was proposer clear staging buffer, 
            reset state to Pending until we get proposing/validating notice from consensus."
        )
    }

    // TODO: pending formal consensus API, guessing the API here to keep things moving.
    // TODO: consider adding block number, it isn't strictly necessary, but will help debugging.
    pub fn validation_start(&mut self) -> L1ProviderResult<()> {
        todo!("Sets internal state as validate, returns error if state is Pending.")
    }

    pub fn proposal_start(&mut self) {
        todo!("Similar to validation_start.")
    }

    /// Simple recovery from L1 and L2 reorgs by reseting the service, which rewinds L1 and L2
    /// information.
    pub fn handle_reorg(&mut self) -> L1ProviderResult<()> {
        self.reset()
    }

    // TODO: this will likely change during integration with infra team.
    pub async fn start(&self) {
        todo!(
            "Create a process that wakes up every config.poll_interval seconds and updates
        internal L1 and L2 buffers according to collected L1 events and recent blocks created on
        L2."
        )
    }

    fn reset(&mut self) -> L1ProviderResult<()> {
        todo!(
            "resets internal buffers and rewinds the internal crawler _pointer_ back for ~1 \
             hour,so that the main loop will start collecting from that time gracefully. May hit \
             base layer errors."
        );
    }
}

pub struct L1ProviderConfig;
