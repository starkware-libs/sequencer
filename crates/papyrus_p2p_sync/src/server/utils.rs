use papyrus_protobuf::sync::{Direction, Query};

use super::P2PSyncServerError;

pub(crate) fn calculate_block_number(
    query: &Query,
    start_block: u64,
    read_blocks_counter: u64,
) -> Result<u64, P2PSyncServerError> {
    let direction_factor: i128 = match query.direction {
        Direction::Forward => 1,
        Direction::Backward => -1,
    };
    // TODO(shahak): Fix this code.
    let blocks_delta: i128 = direction_factor * i128::from(query.step * read_blocks_counter);
    let block_number: i128 = i128::from(start_block) + blocks_delta;

    u64::try_from(block_number).map_err(|_| P2PSyncServerError::BlockNumberOutOfRange {
        query: query.clone(),
        counter: read_blocks_counter,
    })
}
