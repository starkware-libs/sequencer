use futures::StreamExt;
use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use papyrus_network::network_manager::BroadcastTopicChannels;
use papyrus_protobuf::consensus::{ProposalFin, ProposalInit, ProposalPart};
use pretty_assertions::assert_eq;
use rstest::{fixture, rstest};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::{ChainId, ContractAddress};
use starknet_api::felt;
use starknet_api::transaction::TransactionHash;
use starknet_integration_tests::flow_test_setup::FlowTestSetup;
use starknet_integration_tests::utils::{
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
    let mock_running_system = FlowTestSetup::new_from_tx_generator(&tx_generator).await;

    // Create and send transactions.
    let expected_batched_tx_hashes = run_integration_test_scenario(tx_generator, &mut |tx| {
        mock_running_system.assert_add_tx_success(tx)
    })
    .await;
    // TODO(Dan, Itay): Consider adding a utility function that waits for something to happen.
    tokio::time::timeout(
        LISTEN_TO_BROADCAST_MESSAGES_TIMEOUT,
        listen_to_broadcasted_messages(
            mock_running_system.consensus_proposals_channels,
            &expected_batched_tx_hashes,
        ),
    )
    .await
    .expect("listen to broadcasted messages should finish in time");
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
    // TODO (Dan, Guy): retrieve / calculate the expected proposal init and fin.
    let expected_proposal_init = ProposalInit {
        height: BlockNumber(1),
        round: 0,
        valid_round: None,
        proposer: ContractAddress::default(),
    };
    let expected_proposal_fin = ProposalFin {
        proposal_content_id: BlockHash(felt!(
            "0x4597ceedbef644865917bf723184538ef70d43954d63f5b7d8cb9d1bd4c2c32"
        )),
    };
    assert_eq!(
        broadcasted_messages_receiver.next().await.unwrap().0.unwrap(),
        ProposalPart::Init(expected_proposal_init)
    );
    loop {
        match broadcasted_messages_receiver.next().await.unwrap().0.unwrap() {
            ProposalPart::Init(init) => panic!("Unexpected init: {:?}", init),
            ProposalPart::Fin(proposal_fin) => {
                assert_eq!(proposal_fin, expected_proposal_fin);
                break;
            }
            ProposalPart::Transactions(transactions) => {
                received_tx_hashes.extend(
                    transactions
                        .transactions
                        .iter()
                        .map(|tx| tx.calculate_transaction_hash(&chain_id).unwrap()),
                );
            }
        }
    }
    assert_eq!(received_tx_hashes, expected_batched_tx_hashes);
}
