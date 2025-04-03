use std::time::Duration;
use std::vec;

use apollo_network::network_manager::test_utils::{
    mock_register_broadcast_topic,
    MockBroadcastedMessagesSender,
    TestSubscriberChannels,
};
use apollo_network_types::network_types::BroadcastedMessageMetadata;
use apollo_protobuf::consensus::{Vote, DEFAULT_VALIDATOR_ID};
use apollo_test_utils::{get_rng, GetTestInstance};
use futures::channel::{mpsc, oneshot};
use futures::{FutureExt, SinkExt};
use lazy_static::lazy_static;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_types_core::felt::Felt;

use super::{run_consensus, MultiHeightManager, RunHeightRes};
use crate::config::TimeoutsConfig;
use crate::test_utils::{precommit, prevote, proposal_init, MockTestContext, TestProposalPart};
use crate::types::ValidatorId;

lazy_static! {
    static ref PROPOSER_ID: ValidatorId = DEFAULT_VALIDATOR_ID.into();
    static ref VALIDATOR_ID: ValidatorId = (DEFAULT_VALIDATOR_ID + 1).into();
    static ref VALIDATOR_ID_2: ValidatorId = (DEFAULT_VALIDATOR_ID + 2).into();
    static ref VALIDATOR_ID_3: ValidatorId = (DEFAULT_VALIDATOR_ID + 3).into();
    static ref TIMEOUTS: TimeoutsConfig = TimeoutsConfig {
        prevote_timeout: Duration::from_millis(100),
        precommit_timeout: Duration::from_millis(100),
        proposal_timeout: Duration::from_millis(100),
    };
}

const CHANNEL_SIZE: usize = 10;
const SYNC_RETRY_INTERVAL: Duration = Duration::from_millis(100);

async fn send(sender: &mut MockBroadcastedMessagesSender<Vote>, msg: Vote) {
    let broadcasted_message_metadata =
        BroadcastedMessageMetadata::get_test_instance(&mut get_rng());
    sender.send((msg, broadcasted_message_metadata)).await.unwrap();
}

async fn send_proposal(
    proposal_receiver_sender: &mut mpsc::Sender<mpsc::Receiver<TestProposalPart>>,
    content: Vec<TestProposalPart>,
) {
    let (mut proposal_sender, proposal_receiver) = mpsc::channel(CHANNEL_SIZE);
    proposal_receiver_sender.send(proposal_receiver).await.unwrap();
    for item in content {
        proposal_sender.send(item).await.unwrap();
    }
}

fn expect_validate_proposal(context: &mut MockTestContext, block_hash: Felt, times: usize) {
    context
        .expect_validate_proposal()
        .returning(move |_, _, _| {
            let (block_sender, block_receiver) = oneshot::channel();
            block_sender.send(BlockHash(block_hash)).unwrap();
            block_receiver
        })
        .times(times);
}

fn assert_decision(res: RunHeightRes, id: Felt) {
    match res {
        RunHeightRes::Decision(decision) => assert_eq!(decision.block, BlockHash(id)),
        _ => panic!("Expected decision"),
    }
}

#[tokio::test]
async fn manager_multiple_heights_unordered() {
    let TestSubscriberChannels { mock_network, subscriber_channels } =
        mock_register_broadcast_topic().unwrap();
    let mut sender = mock_network.broadcasted_messages_sender;

    let (mut proposal_receiver_sender, mut proposal_receiver_receiver) =
        mpsc::channel(CHANNEL_SIZE);

    // Send messages for height 2 followed by those for height 1.
    send_proposal(
        &mut proposal_receiver_sender,
        vec![TestProposalPart::Init(proposal_init(2, 0, *PROPOSER_ID))],
    )
    .await;
    send(&mut sender, prevote(Some(Felt::TWO), 2, 0, *PROPOSER_ID)).await;
    send(&mut sender, precommit(Some(Felt::TWO), 2, 0, *PROPOSER_ID)).await;

    send_proposal(
        &mut proposal_receiver_sender,
        vec![TestProposalPart::Init(proposal_init(1, 0, *PROPOSER_ID))],
    )
    .await;
    send(&mut sender, prevote(Some(Felt::ONE), 1, 0, *PROPOSER_ID)).await;
    send(&mut sender, precommit(Some(Felt::ONE), 1, 0, *PROPOSER_ID)).await;

    let mut context = MockTestContext::new();
    // Run the manager for height 1.
    context.expect_try_sync().returning(|_| false);
    expect_validate_proposal(&mut context, Felt::ONE, 1);
    context.expect_validators().returning(move |_| vec![*PROPOSER_ID, *VALIDATOR_ID]);
    context.expect_proposer().returning(move |_, _| *PROPOSER_ID);
    context.expect_set_height_and_round().returning(move |_, _| ());
    context.expect_broadcast().returning(move |_| Ok(()));

    let mut manager = MultiHeightManager::new(*VALIDATOR_ID, TIMEOUTS.clone());
    let mut subscriber_channels = subscriber_channels.into();
    let decision = manager
        .run_height(
            &mut context,
            BlockNumber(1),
            false,
            SYNC_RETRY_INTERVAL,
            &mut subscriber_channels,
            &mut proposal_receiver_receiver,
        )
        .await
        .unwrap();
    assert_decision(decision, Felt::ONE);

    // Run the manager for height 2.
    expect_validate_proposal(&mut context, Felt::TWO, 1);
    let decision = manager
        .run_height(
            &mut context,
            BlockNumber(2),
            false,
            SYNC_RETRY_INTERVAL,
            &mut subscriber_channels,
            &mut proposal_receiver_receiver,
        )
        .await
        .unwrap();
    assert_decision(decision, Felt::TWO);
}

#[tokio::test]
async fn run_consensus_sync() {
    // Set expectations.
    let mut context = MockTestContext::new();
    let (decision_tx, decision_rx) = oneshot::channel();

    let (mut proposal_receiver_sender, proposal_receiver_receiver) = mpsc::channel(CHANNEL_SIZE);

    expect_validate_proposal(&mut context, Felt::TWO, 1);
    context.expect_validators().returning(move |_| vec![*PROPOSER_ID, *VALIDATOR_ID]);
    context.expect_proposer().returning(move |_, _| *PROPOSER_ID);
    context.expect_set_height_and_round().returning(move |_, _| ());
    context.expect_broadcast().returning(move |_| Ok(()));
    context
        .expect_decision_reached()
        .withf(move |block, votes| *block == BlockHash(Felt::TWO) && votes[0].height == 2)
        .return_once(move |_, _| {
            decision_tx.send(()).unwrap();
            Ok(())
        });
    context
        .expect_try_sync()
        .withf(move |height| *height == BlockNumber(1))
        .times(1)
        .returning(|_| true);
    context.expect_try_sync().returning(|_| false);

    // Send messages for height 2.
    send_proposal(
        &mut proposal_receiver_sender,
        vec![TestProposalPart::Init(proposal_init(2, 0, *PROPOSER_ID))],
    )
    .await;
    let TestSubscriberChannels { mock_network, subscriber_channels } =
        mock_register_broadcast_topic().unwrap();
    let mut network_sender = mock_network.broadcasted_messages_sender;
    send(&mut network_sender, prevote(Some(Felt::TWO), 2, 0, *PROPOSER_ID)).await;
    send(&mut network_sender, precommit(Some(Felt::TWO), 2, 0, *PROPOSER_ID)).await;

    // Start at height 1.
    tokio::spawn(async move {
        run_consensus(
            context,
            BlockNumber(1),
            BlockNumber(1),
            *VALIDATOR_ID,
            Duration::ZERO,
            TIMEOUTS.clone(),
            SYNC_RETRY_INTERVAL,
            subscriber_channels.into(),
            proposal_receiver_receiver,
        )
        .await
    });

    // Decision for height 2.
    decision_rx.await.unwrap();
}

#[tokio::test]
async fn test_timeouts() {
    let TestSubscriberChannels { mock_network, subscriber_channels } =
        mock_register_broadcast_topic().unwrap();
    let mut sender = mock_network.broadcasted_messages_sender;

    let (mut proposal_receiver_sender, mut proposal_receiver_receiver) =
        mpsc::channel(CHANNEL_SIZE);

    send_proposal(
        &mut proposal_receiver_sender,
        vec![TestProposalPart::Init(proposal_init(1, 0, *PROPOSER_ID))],
    )
    .await;
    send(&mut sender, prevote(None, 1, 0, *VALIDATOR_ID_2)).await;
    send(&mut sender, prevote(None, 1, 0, *VALIDATOR_ID_3)).await;
    send(&mut sender, precommit(None, 1, 0, *VALIDATOR_ID_2)).await;
    send(&mut sender, precommit(None, 1, 0, *VALIDATOR_ID_3)).await;

    let mut context = MockTestContext::new();
    context.expect_set_height_and_round().returning(move |_, _| ());
    expect_validate_proposal(&mut context, Felt::ONE, 2);
    context
        .expect_validators()
        .returning(move |_| vec![*PROPOSER_ID, *VALIDATOR_ID, *VALIDATOR_ID_2, *VALIDATOR_ID_3]);
    context.expect_proposer().returning(move |_, _| *PROPOSER_ID);
    context.expect_try_sync().returning(|_| false);

    let (timeout_send, timeout_receive) = oneshot::channel();
    // Node handled Timeout events and responded with NIL vote.
    context
        .expect_broadcast()
        .times(1)
        .withf(move |msg: &Vote| msg == &prevote(None, 1, 1, *VALIDATOR_ID))
        .return_once(move |_| {
            timeout_send.send(()).unwrap();
            Ok(())
        });
    context.expect_broadcast().returning(move |_| Ok(()));

    let mut manager = MultiHeightManager::new(*VALIDATOR_ID, TIMEOUTS.clone());
    let manager_handle = tokio::spawn(async move {
        let decision = manager
            .run_height(
                &mut context,
                BlockNumber(1),
                false,
                SYNC_RETRY_INTERVAL,
                &mut subscriber_channels.into(),
                &mut proposal_receiver_receiver,
            )
            .await
            .unwrap();
        assert_decision(decision, Felt::ONE);
    });

    // Wait for the timeout to be triggered.
    timeout_receive.await.unwrap();
    // Show that after the timeout is triggered we can still precommit in favor of the block and
    // reach a decision.
    send_proposal(
        &mut proposal_receiver_sender,
        vec![TestProposalPart::Init(proposal_init(1, 1, *PROPOSER_ID))],
    )
    .await;
    send(&mut sender, prevote(Some(Felt::ONE), 1, 1, *PROPOSER_ID)).await;
    send(&mut sender, prevote(Some(Felt::ONE), 1, 1, *VALIDATOR_ID_2)).await;
    send(&mut sender, prevote(Some(Felt::ONE), 1, 1, *VALIDATOR_ID_3)).await;
    send(&mut sender, precommit(Some(Felt::ONE), 1, 1, *VALIDATOR_ID_2)).await;
    send(&mut sender, precommit(Some(Felt::ONE), 1, 1, *VALIDATOR_ID_3)).await;

    manager_handle.await.unwrap();
}

#[tokio::test]
async fn timely_message_handling() {
    // TODO(matan): Make run_height more generic so don't need mock network?
    // Check that, even when sync is immediately ready, consensus still handles queued messages.
    let mut context = MockTestContext::new();
    context.expect_try_sync().returning(|_| true);

    // Send messages
    let (mut proposal_receiver_sender, mut proposal_receiver_receiver) = mpsc::channel(0);
    let (mut content_sender, content_receiver) = mpsc::channel(0);
    content_sender.try_send(TestProposalPart::Init(proposal_init(1, 0, *PROPOSER_ID))).unwrap();
    proposal_receiver_sender.try_send(content_receiver).unwrap();

    // Fill up the sender.
    let TestSubscriberChannels { mock_network, subscriber_channels } =
        mock_register_broadcast_topic().unwrap();
    let mut subscriber_channels = subscriber_channels.into();
    let mut vote_sender = mock_network.broadcasted_messages_sender;
    let metadata = BroadcastedMessageMetadata::get_test_instance(&mut get_rng());
    let vote = prevote(Some(Felt::TWO), 1, 0, *PROPOSER_ID);
    // Fill up the buffer.
    while vote_sender.send((vote.clone(), metadata.clone())).now_or_never().is_some() {}

    let mut manager = MultiHeightManager::new(*VALIDATOR_ID, TIMEOUTS.clone());
    let res = manager
        .run_height(
            &mut context,
            BlockNumber(1),
            false,
            SYNC_RETRY_INTERVAL,
            &mut subscriber_channels,
            &mut proposal_receiver_receiver,
        )
        .await;
    assert_eq!(res, Ok(RunHeightRes::Sync));

    // Try sending another proposal, to check that, even though sync was known at the beginning of
    // the height and so consensus was not actually run, the inbound channels are cleared.
    proposal_receiver_sender.try_send(mpsc::channel(1).1).unwrap();
    assert!(vote_sender.send((vote.clone(), metadata.clone())).now_or_never().is_some());
}
