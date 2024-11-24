use std::collections::HashSet;

use futures::StreamExt;
use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use papyrus_network::network_manager::BroadcastTopicChannels;
use papyrus_protobuf::consensus::{ProposalFin, ProposalInit, ProposalPart};
use papyrus_storage::test_utils::CHAIN_ID_FOR_TESTS;
use pretty_assertions::assert_eq;
use rstest::{fixture, rstest};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::ContractAddress;
use starknet_api::transaction::TransactionHash;
use starknet_integration_tests::flow_test_setup::FlowTestSetup;
use starknet_integration_tests::utils::{
    create_integration_test_tx_generator,
    run_integration_test_scenario,
};
use starknet_types_core::felt::Felt;
use tracing::debug;

const INITIAL_HEIGHT: BlockNumber = BlockNumber(0);
const LAST_HEIGHT: BlockNumber = BlockNumber(3);

#[fixture]
fn tx_generator() -> MultiAccountTransactionGenerator {
    create_integration_test_tx_generator()
}

#[rstest]
#[tokio::test]
async fn end_to_end(mut tx_generator: MultiAccountTransactionGenerator) {
    const LISTEN_TO_BROADCAST_MESSAGES_TIMEOUT: std::time::Duration =
        std::time::Duration::from_secs(5);
    // Setup.
    let mut mock_running_system = FlowTestSetup::new_from_tx_generator(&tx_generator).await;

    let next_height = INITIAL_HEIGHT.unchecked_next();
    let expected_content_ids = [
        Felt::from_hex_unchecked(
            "0x4597ceedbef644865917bf723184538ef70d43954d63f5b7d8cb9d1bd4c2c32",
        ),
        Felt::from_hex_unchecked(
            "0x7e2c0e448bea6bbf00962017d8addd56c6146d5beb5a273b2e02f5fb862d20f",
        ),
    ];

    // Buld multiple heights to ensure heights are committed.
    for (height, expected_content_id) in
        next_height.iter_up_to(LAST_HEIGHT.unchecked_next()).zip(expected_content_ids.iter())
    {
        debug!("Starting height {}.", height);
        // Create and send transactions.
        let expected_batched_tx_hashes =
            run_integration_test_scenario(&mut tx_generator, &mut |tx| {
                mock_running_system.assert_add_tx_success(tx)
            })
            .await;
        // TODO(Dan, Itay): Consider adding a utility function that waits for something to happen.
        tokio::time::timeout(
            LISTEN_TO_BROADCAST_MESSAGES_TIMEOUT,
            listen_to_broadcasted_messages(
                &mut mock_running_system.consensus_proposals_channels,
                &expected_batched_tx_hashes,
                height,
                *expected_content_id,
            ),
        )
        .await
        .expect("listen to broadcasted messages should finish in time");
    }
}

async fn listen_to_broadcasted_messages(
    consensus_proposals_channels: &mut BroadcastTopicChannels<ProposalPart>,
    expected_batched_tx_hashes: &[TransactionHash],
    expected_height: BlockNumber,
    expected_content_id: Felt,
) {
    let chain_id = CHAIN_ID_FOR_TESTS.clone();
    let broadcasted_messages_receiver =
        &mut consensus_proposals_channels.broadcasted_messages_receiver;
    let mut received_tx_hashes = HashSet::new();
    // TODO (Dan, Guy): retrieve / calculate the expected proposal init and fin.
    let expected_proposal_init = ProposalInit {
        height: expected_height,
        round: 0,
        valid_round: None,
        proposer: ContractAddress::default(),
    };
    let expected_proposal_fin = ProposalFin { proposal_content_id: BlockHash(expected_content_id) };
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
    // Using HashSet to ignore the order of the transactions (broadcast can lead to reordering).
    assert_eq!(
        received_tx_hashes,
        expected_batched_tx_hashes.iter().cloned().collect::<HashSet<_>>()
    );
}
