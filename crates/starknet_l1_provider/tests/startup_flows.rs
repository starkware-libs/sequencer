#[test]
#[ignore = "Mock the spawn initialization task for deterministic testing, then test a scenario \
            with several commits blocks, some applied, some backlogged, some applied and trigger \
            backlog consumption."]
fn backlog_happy_flow() {
    todo!()
}

#[test]
#[ignore = "similar to backlog_happy_flow, only shorter, and sprinkle some start_block/get_txs \
            attemps while its bootstarpping (and assert failure on height), then assert that they \
            succeed after bootstrapping ends."]
fn bootstrap_completion() {
    todo!()
}
