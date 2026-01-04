use alloy::node_bindings::Anvil;
use papyrus_base_layer::ethereum_base_layer_contract::{
    EthereumBaseLayerConfig,
    EthereumBaseLayerContract,
    EthereumBaseLayerError,
};
use papyrus_base_layer::test_utils::{
    make_block_history_on_anvil,
    ARBITRARY_ANVIL_L1_ACCOUNT_ADDRESS,
    OTHER_ARBITRARY_ANVIL_L1_ACCOUNT_ADDRESS,
};
use papyrus_base_layer::BaseLayerContract;

#[tokio::test]
async fn anvil_starts_with_no_contract() {
    const NUM_L1_TRANSACTIONS: usize = 10;
    // TODO(GuyNir/Shahak): avoid this hard-coded port number, and align port usages throughout the
    // anvil instances.
    let anvil = Anvil::new()
        .port(9999_u16)
        .try_spawn()
        .expect("Anvil not installed, see anvil base layer for installation instructions.");
    let base_layer_config = EthereumBaseLayerConfig {
        ordered_l1_endpoint_urls: vec![anvil.endpoint_url().into()],
        ..Default::default()
    };
    let mut base_layer = EthereumBaseLayerContract::new(base_layer_config.clone());

    let sender_address = ARBITRARY_ANVIL_L1_ACCOUNT_ADDRESS;
    let receiver_address = OTHER_ARBITRARY_ANVIL_L1_ACCOUNT_ADDRESS;
    make_block_history_on_anvil(
        sender_address,
        receiver_address,
        base_layer_config.clone(),
        NUM_L1_TRANSACTIONS,
    )
    .await;

    let latest_l1_block_number = base_layer.latest_l1_block_number().await.unwrap();
    assert_eq!(latest_l1_block_number, u64::try_from(NUM_L1_TRANSACTIONS).unwrap());

    let latest_proved_block = base_layer.get_proved_block_at(latest_l1_block_number).await;
    // In case L1 contains blocks but does not contain a contract, we get Overrun error.
    // TODO(guyn): We never get Ok(None) from latest_proved_block, we should remove that option.
    assert_eq!(
        latest_proved_block,
        Err(EthereumBaseLayerError::TypeError(alloy::sol_types::Error::Overrun))
    );
}
