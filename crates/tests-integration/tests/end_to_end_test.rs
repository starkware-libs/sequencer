use futures::StreamExt;
use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use papyrus_network::network_manager::BroadcastTopicChannels;
use papyrus_protobuf::consensus::ProposalPart;
use pretty_assertions::assert_eq;
use rstest::{fixture, rstest};
use starknet_api::core::ChainId;
use starknet_api::transaction::TransactionHash;
use starknet_integration_tests::integration_test_setup::IntegrationTestSetup;
use starknet_integration_tests::integration_test_utils::{
    create_integration_test_tx_generator,
    run_integration_test_scenario,
};

#[fixture]
fn tx_generator() -> MultiAccountTransactionGenerator {
    create_integration_test_tx_generator()
}

#[rstest]
#[tokio::test]
async fn end_to_end(tx_generator: MultiAccountTransactionGenerator) {
    const LISTEN_TO_BROADCAST_MESSAGES_TIMEOUT: std::time::Duration =
        std::time::Duration::from_secs(5);
    // Setup.
    let mock_running_system = IntegrationTestSetup::new_from_tx_generator(&tx_generator).await;

    // Create and send transactions.
    let expected_batched_tx_hashes = run_integration_test_scenario(tx_generator, &|tx| {
        mock_running_system.assert_add_tx_success(tx)
    })
    .await;
    // TODO(Dan, Itay): Consider adding a utility function that waits for something to happen.
    let join_handle = tokio::spawn(async move {
        tokio::time::timeout(
            LISTEN_TO_BROADCAST_MESSAGES_TIMEOUT,
            listen_to_broadcasted_messages(
                mock_running_system.consensus_proposals_channels,
                &expected_batched_tx_hashes,
            ),
        )
        .await
        .expect("listen to broadcasted messages should finish in time");
    });
    join_handle.await.expect("Task should succeed");
}

async fn listen_to_broadcasted_messages(
    consensus_proposals_channels: BroadcastTopicChannels<ProposalPart>,
    expected_batched_tx_hashes: &[TransactionHash],
) {
    // TODO(Dan, Guy): retrieve chain ID. Maybe by modifying IntegrationTestSetup to hold it as a
    // member, and instantiate the value using StorageTestSetup.
    const CHAIN_ID_NAME: &str = "CHAIN_ID_SUBDIR";
    let chain_id = ChainId::Other(CHAIN_ID_NAME.to_string());
    let mut broadcasted_messages_receiver =
        consensus_proposals_channels.broadcasted_messages_receiver;
    let mut received_tx_hashes = vec![];
    while received_tx_hashes.len() < expected_batched_tx_hashes.len() {
        let (message, _broadcasted_message_metadata) = broadcasted_messages_receiver
            .next()
            .await
            .unwrap_or_else(|| panic!("Expected to receive a message from the broadcast topic"));
        if let ProposalPart::Transactions(transactions) = message.unwrap() {
            received_tx_hashes.append(
                &mut transactions
                    .transactions
                    .iter()
                    .map(|tx| tx.calculate_transaction_hash(&chain_id).unwrap())
                    .collect(),
            );
        }
    }
    assert_eq!(received_tx_hashes, expected_batched_tx_hashes);
}
