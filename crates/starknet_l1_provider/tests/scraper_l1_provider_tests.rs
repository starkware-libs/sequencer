use starknet_l1_provider::{l1_provider::create_l1_provider, L1ProviderConfig};

#[test]
#[ignore = "Mock the spawn initialization task for deterministic testing, then test a scenario \
            with several commits blocks, some applied, some backlogged, some applied and trigger \
            backlog consumption."]
fn bootstrap_e2e() {
    let config = L1ProviderConfig {
        _poll_interval: todo!(),
        provider_startup_height: todo!(),
        bootstrap_catch_up_height: todo!(),
    };
    let l1_provider = create_l1_provider(config, l1_provider_client, sync_client)
}

#[test]
#[ignore = "similar to backlog_happy_flow, only shorter, and sprinkle some start_block/get_txs \
            attemps while its bootstarpping (and assert failure on height), then assert that they \
            succeed after bootstrapping ends."]
fn bootstrap_completion() {
    todo!()
}
