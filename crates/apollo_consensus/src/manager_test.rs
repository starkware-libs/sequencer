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
use crate::config::{FutureMsgLimit, TimeoutsConfig};
use crate::test_utils::{precommit, prevote, proposal_init, MockTestContext, TestProposalPart};
use crate::types::ValidatorId;
use crate::votes_threshold::QuorumType;
use crate::RunConsensusArguments;

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
const FUTURE_MSG_LIMIT: FutureMsgLimit = FutureMsgLimit {
    future_height_limit: 10,
    future_round_limit: 10,
    future_height_round_limit: 1,
};

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

fn assert_decision(res: RunHeightRes, id: Felt, round: u32) {
    match res {
        RunHeightRes::Decision(decision) => {
            assert_eq!(decision.block, BlockHash(id));
            assert_eq!(decision.precommits[0].round, round);
        }
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

    let mut manager = MultiHeightManager::new(
        *VALIDATOR_ID,
        SYNC_RETRY_INTERVAL,
        QuorumType::Byzantine,
        TIMEOUTS.clone(),
        FUTURE_MSG_LIMIT,
    );
    let mut subscriber_channels = subscriber_channels.into();
    let decision = manager
        .run_height(
            &mut context,
            BlockNumber(1),
            false,
            &mut subscriber_channels,
            &mut proposal_receiver_receiver,
        )
        .await
        .unwrap();
    assert_decision(decision, Felt::ONE, 0);

    // Run the manager for height 2.
    expect_validate_proposal(&mut context, Felt::TWO, 1);
    let decision = manager
        .run_height(
            &mut context,
            BlockNumber(2),
            false,
            &mut subscriber_channels,
            &mut proposal_receiver_receiver,
        )
        .await
        .unwrap();
    assert_decision(decision, Felt::TWO, 0);
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
    let run_consensus_args = RunConsensusArguments {
        start_active_height: BlockNumber(1),
        start_observe_height: BlockNumber(1),
        validator_id: *VALIDATOR_ID,
        consensus_delay: Duration::ZERO,
        timeouts: TIMEOUTS.clone(),
        sync_retry_interval: SYNC_RETRY_INTERVAL,
        quorum_type: QuorumType::Byzantine,
        future_msg_limit: FUTURE_MSG_LIMIT,
    };
    // Start at height 1.
    tokio::spawn(async move {
        run_consensus(
            run_consensus_args,
            context,
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

    let mut manager = MultiHeightManager::new(
        *VALIDATOR_ID,
        SYNC_RETRY_INTERVAL,
        QuorumType::Byzantine,
        TIMEOUTS.clone(),
        FUTURE_MSG_LIMIT,
    );
    let manager_handle = tokio::spawn(async move {
        let decision = manager
            .run_height(
                &mut context,
                BlockNumber(1),
                false,
                &mut subscriber_channels.into(),
                &mut proposal_receiver_receiver,
            )
            .await
            .unwrap();
        assert_decision(decision, Felt::ONE, 1);
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

    let mut manager = MultiHeightManager::new(
        *VALIDATOR_ID,
        SYNC_RETRY_INTERVAL,
        QuorumType::Byzantine,
        TIMEOUTS.clone(),
        FUTURE_MSG_LIMIT,
    );
    let res = manager
        .run_height(
            &mut context,
            BlockNumber(1),
            false,
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

#[tokio::test]
async fn future_height_limit_caching_and_dropping() {
    // Use very low limit - only cache 1 height ahead with round 0.
    const LOW_HEIGHT_LIMIT: u32 = 1;
    const LOW_ROUND_LIMIT: u32 = 0;
    const LOW_HEIGHT_ROUND_LIMIT: u32 = 0;

    let TestSubscriberChannels { mock_network, subscriber_channels } =
        mock_register_broadcast_topic().unwrap();
    let mut sender = mock_network.broadcasted_messages_sender;

    let (mut proposal_receiver_sender, mut proposal_receiver_receiver) =
        mpsc::channel(CHANNEL_SIZE);

    // Send proposal and votes for height 2 (should be dropped when processing height 0).
    send_proposal(
        &mut proposal_receiver_sender,
        vec![TestProposalPart::Init(proposal_init(2, 0, *PROPOSER_ID))],
    )
    .await;
    send(&mut sender, prevote(Some(Felt::TWO), 2, 0, *PROPOSER_ID)).await;
    send(&mut sender, precommit(Some(Felt::TWO), 2, 0, *PROPOSER_ID)).await;

    // Send proposal and votes for height 1 (should be cached when processing height 0).
    send_proposal(
        &mut proposal_receiver_sender,
        vec![TestProposalPart::Init(proposal_init(1, 0, *PROPOSER_ID))],
    )
    .await;
    send(&mut sender, prevote(Some(Felt::ONE), 1, 0, *PROPOSER_ID)).await;
    send(&mut sender, precommit(Some(Felt::ONE), 1, 0, *PROPOSER_ID)).await;

    // Send proposal and votes for height 0 (current height - needed to reach consensus).
    send_proposal(
        &mut proposal_receiver_sender,
        vec![TestProposalPart::Init(proposal_init(0, 0, *PROPOSER_ID))],
    )
    .await;
    send(&mut sender, prevote(Some(Felt::ZERO), 0, 0, *PROPOSER_ID)).await;
    send(&mut sender, precommit(Some(Felt::ZERO), 0, 0, *PROPOSER_ID)).await;

    let mut context = MockTestContext::new();
    context.expect_try_sync().returning(|_| false);
    expect_validate_proposal(&mut context, Felt::ZERO, 1); // Height 0 validation
    expect_validate_proposal(&mut context, Felt::ONE, 1); // Height 1 validation
    context.expect_validators().returning(move |_| vec![*PROPOSER_ID, *VALIDATOR_ID]);
    context.expect_proposer().returning(move |_, _| *PROPOSER_ID);
    context.expect_set_height_and_round().returning(move |_, _| ());
    // Set up coordination to detect when node votes Nil for height 2 (indicating proposal was
    // dropped, so the node didn't received the proposal and votes Nil).
    let (height2_nil_vote_trigger, height2_nil_vote_wait) = oneshot::channel();
    context
        .expect_broadcast()
        .withf(move |vote: &Vote| vote.height == 2 && vote.block_hash.is_none())
        .times(1)
        .return_once(move |_| {
            height2_nil_vote_trigger.send(()).unwrap();
            Ok(())
        });
    // Handle all other broadcasts normally.
    context.expect_broadcast().returning(move |_| Ok(()));

    let mut manager = MultiHeightManager::new(
        *VALIDATOR_ID,
        SYNC_RETRY_INTERVAL,
        QuorumType::Byzantine,
        TIMEOUTS.clone(),
        FutureMsgLimit {
            future_height_limit: LOW_HEIGHT_LIMIT,
            future_round_limit: LOW_ROUND_LIMIT,
            future_height_round_limit: LOW_HEIGHT_ROUND_LIMIT,
        },
    );
    let mut subscriber_channels = subscriber_channels.into();

    // Run height 0 - should drop height 2 messages, cache height 1 messages, and reach consensus.
    let decision = manager
        .run_height(
            &mut context,
            BlockNumber(0),
            false,
            &mut subscriber_channels,
            &mut proposal_receiver_receiver,
        )
        .await
        .unwrap();
    assert_decision(decision, Felt::ZERO, 0);

    // Run height 1 - should succeed using cached proposal.
    let decision = manager
        .run_height(
            &mut context,
            BlockNumber(1),
            false,
            &mut subscriber_channels,
            &mut proposal_receiver_receiver,
        )
        .await
        .unwrap();
    assert_decision(decision, Felt::ONE, 0);

    // Run height 2 in background - shouldn't reach consensus because proposal was dropped.
    let manager_handle = tokio::spawn(async move {
        manager
            .run_height(
                &mut context,
                BlockNumber(2),
                false,
                &mut subscriber_channels,
                &mut proposal_receiver_receiver,
            )
            .await
    });

    // Race between consensus completing and height2_nil_vote_trigger being fired.
    tokio::select! {
        _ = height2_nil_vote_wait => {
            // SUCCESS: height2_nil_vote_trigger was fired - this means the proposal was dropped as
            // expected, and the node didn't receive the proposal and votes Nil.
        }
        consensus_result = manager_handle => {
            panic!("FAIL: Node should not reach consensus. {consensus_result:?}");
        }
    }
}

#[tokio::test]
async fn current_height_round_limit_caching_and_dropping() {
    const HEIGHT_LIMIT: u32 = 10;
    const LOW_ROUND_LIMIT: u32 = 0; // Accept only current round (current_round + 0).
    const HEIGHT_ROUND_LIMIT: u32 = 1;

    let TestSubscriberChannels { mock_network, subscriber_channels } =
        mock_register_broadcast_topic().unwrap();
    let mut sender = mock_network.broadcasted_messages_sender;

    let (mut proposal_receiver_sender, mut proposal_receiver_receiver) =
        mpsc::channel(CHANNEL_SIZE);

    // Send proposals for rounds 0 and 1, proposal for round 1 should be dropped.
    send_proposal(
        &mut proposal_receiver_sender,
        vec![TestProposalPart::Init(proposal_init(1, 0, *PROPOSER_ID))],
    )
    .await;
    send_proposal(
        &mut proposal_receiver_sender,
        vec![TestProposalPart::Init(proposal_init(1, 1, *PROPOSER_ID))],
    )
    .await;

    // Send votes for round 1. These should be dropped because when state machine is in round 0,
    // round 1 > current_round(0) + future_round_limit(0).
    send(&mut sender, prevote(Some(Felt::ONE), 1, 1, *PROPOSER_ID)).await;
    send(&mut sender, prevote(Some(Felt::ONE), 1, 1, *VALIDATOR_ID_2)).await;
    send(&mut sender, precommit(Some(Felt::ONE), 1, 1, *PROPOSER_ID)).await;
    send(&mut sender, precommit(Some(Felt::ONE), 1, 1, *VALIDATOR_ID_2)).await;

    // Send Nil votes for round 0 (current round).
    send(&mut sender, prevote(None, 1, 0, *VALIDATOR_ID_2)).await;
    send(&mut sender, prevote(None, 1, 0, *PROPOSER_ID)).await;
    send(&mut sender, precommit(None, 1, 0, *VALIDATOR_ID_2)).await;
    send(&mut sender, precommit(None, 1, 0, *PROPOSER_ID)).await;

    let mut context = MockTestContext::new();
    context.expect_try_sync().returning(|_| false);
    // Will be called twice for round 0 and 2 (will send the proposal when advancing to round 2).
    expect_validate_proposal(&mut context, Felt::ONE, 2);
    context
        .expect_validators()
        .returning(move |_| vec![*PROPOSER_ID, *VALIDATOR_ID, *VALIDATOR_ID_2]);
    context.expect_proposer().returning(move |_, _| *PROPOSER_ID);
    context.expect_broadcast().returning(move |_| Ok(()));

    // Set up coordination for round advancement.
    let (round1_trigger, round1_wait) = oneshot::channel();
    let (round2_trigger, round2_wait) = oneshot::channel();

    context
        .expect_set_height_and_round()
        .withf(|height, round| *height == BlockNumber(1) && *round == 1)
        .times(1)
        .return_once(|_, _| {
            round1_trigger.send(()).unwrap();
        });
    context
        .expect_set_height_and_round()
        .withf(|height, round| *height == BlockNumber(1) && *round == 2)
        .times(1)
        .return_once(|_, _| {
            round2_trigger.send(()).unwrap();
        });
    // Handle all other set_height_and_round calls normally.
    context.expect_set_height_and_round().returning(move |_, _| ());

    let mut manager = MultiHeightManager::new(
        *VALIDATOR_ID,
        SYNC_RETRY_INTERVAL,
        QuorumType::Byzantine,
        TIMEOUTS.clone(),
        FutureMsgLimit {
            future_height_limit: HEIGHT_LIMIT,
            future_round_limit: LOW_ROUND_LIMIT,
            future_height_round_limit: HEIGHT_ROUND_LIMIT,
        },
    );
    let mut subscriber_channels = subscriber_channels.into();

    // Spawn tasks to send messages when rounds advance.
    let mut sender_clone1 = sender.clone();
    tokio::spawn(async move {
        round1_wait.await.unwrap();
        // Send Nil votes from other nodes for round 1.
        send(&mut sender_clone1, prevote(None, 1, 1, *VALIDATOR_ID_2)).await;
        send(&mut sender_clone1, prevote(None, 1, 1, *PROPOSER_ID)).await;
        send(&mut sender_clone1, precommit(None, 1, 1, *VALIDATOR_ID_2)).await;
        send(&mut sender_clone1, precommit(None, 1, 1, *PROPOSER_ID)).await;
    });

    let mut sender_clone2 = sender.clone();
    let mut proposal_sender_clone = proposal_receiver_sender.clone();
    tokio::spawn(async move {
        round2_wait.await.unwrap();
        // Send proposal for round 2.
        send_proposal(
            &mut proposal_sender_clone,
            vec![TestProposalPart::Init(proposal_init(1, 2, *PROPOSER_ID))],
        )
        .await;
        // Send votes for round 2.
        send(&mut sender_clone2, prevote(Some(Felt::ONE), 1, 2, *PROPOSER_ID)).await;
        send(&mut sender_clone2, prevote(Some(Felt::ONE), 1, 2, *VALIDATOR_ID_2)).await;
        send(&mut sender_clone2, precommit(Some(Felt::ONE), 1, 2, *PROPOSER_ID)).await;
        send(&mut sender_clone2, precommit(Some(Felt::ONE), 1, 2, *VALIDATOR_ID_2)).await;
    });

    // Run height 1 - should reach consensus in round 2 because:
    // 1. Round 1 votes (sent initially) are dropped since 1 > current_round(0) +
    //    future_round_limit(0)
    // 2. Round 0 has Nil votes → timeout → advance to round 1
    // 3. When advancing to round 1, send Nil votes for round 1 → timeout → advance to round 2
    // 4. When advancing to round 2, send proposal + quorum votes for round 2 → consensus reached
    let decision = manager
        .run_height(
            &mut context,
            BlockNumber(1),
            false,
            &mut subscriber_channels,
            &mut proposal_receiver_receiver,
        )
        .await
        .unwrap();
    assert_decision(decision, Felt::ONE, 2);
}
