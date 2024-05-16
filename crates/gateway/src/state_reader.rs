use blockifier::blockifier::block::BlockInfo;
use blockifier::state::errors::StateError;
use blockifier::state::state_api::StateReader as BlockifierStateReader;
use starknet_api::block::BlockNumber;

pub trait MempoolStateReader: BlockifierStateReader {
    fn get_block_info(&self) -> Result<BlockInfo, StateError>;
}

pub trait StateReaderFactory<T: MempoolStateReader> {
    fn get_state_reader_from_latest_block(&self) -> T;
    fn get_state_reader(&self, block_number: BlockNumber) -> T;
}
