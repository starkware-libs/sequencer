pub async fn revert_blocks_and_eternal_pending(
    mut current_block_number: BlockNumber,
    revert_up_to_and_including: BlockNumber,
    revert_block_fn: impl FnMut(BlockNumber) -> impl Future<Output = ()>,
    component_name: &str,
) -> Never {
    if current_block_number <= revert_up_to_and_including {
        panic!(
            "{component_name} current block {current_block_number} is not larger than the target \
             block {revert_up_to_and_including}. No reverts are needed."
        );
    }

    info!(
        "Reverting {component_name} from block {current_block_number} to block \
         {revert_up_to_and_including}"
    );

    while current_block_number > revert_up_to_and_including {
        revert_block_fn(current_block_number).await;
        current_block_number = current_block_number.prev().expect(
            "A block number that's greater than another block number should return Some on prev",
        );
    }
}
