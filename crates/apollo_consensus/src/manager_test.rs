use std::sync::Arc;
use std::time::Duration;
use std::vec;

use apollo_batcher_types::communication::BatcherClientError;
use apollo_batcher_types::errors::BatcherError;
use apollo_config_manager_types::communication::MockConfigManagerClient;
use apollo_consensus_config::config::{
    ConsensusConfig,
    ConsensusDynamicConfig,
    ConsensusStaticConfig,
    FutureMsgLimitsConfig,
    Timeout,
    TimeoutsConfig,
};
use apollo_network::network_manager::test_utils::{
    mock_register_broadcast_topic,
    MockBroadcastedMessagesSender,
    TestSubscriberChannels,
};
use apollo_network_types::network_types::BroadcastedMessageMetadata;
use apollo_protobuf::consensus::{ProposalCommitment, Vote, DEFAULT_VALIDATOR_ID};
use apollo_storage::StorageConfig;
use apollo_test_utils::{get_rng, GetTestInstance};
use futures::channel::{mpsc, oneshot};
use futures::{FutureExt, SinkExt};
use lazy_static::lazy_static;
use mockall::predicate::eq;
use mockall::Sequence;
use rstest::{fixture, rstest};
use starknet_api::block::BlockNumber;
use starknet_types_core::felt::Felt;
use tokio::sync::Mutex;

use super::{run_consensus, MultiHeightManager, RunHeightRes};
use crate::storage::MockHeightVotedStorageTrait;
use crate::test_utils::{
    block_info,
    precommit,
    prevote,
    MockTestContext,
    NoOpHeightVotedStorage,
    TestProposalPart,
};
use crate::types::{ConsensusError, Round, ValidatorId};
use crate::votes_threshold::QuorumType;
use crate::RunConsensusArguments;

lazy_static! {
    static ref PROPOSER_ID: ValidatorId = DEFAULT_VALIDATOR_ID.into();
    static ref VALIDATOR_ID: ValidatorId = (DEFAULT_VALIDATOR_ID + 1).into();
    static ref VALIDATOR_ID_2: ValidatorId = (DEFAULT_VALIDATOR_ID + 2).into();
    static ref VALIDATOR_ID_3: ValidatorId = (DEFAULT_VALIDATOR_ID + 3).into();
    static ref TIMEOUTS: TimeoutsConfig = TimeoutsConfig::new(
        // proposal
        Timeout::new(
            Duration::from_millis(100),
            Duration::from_millis(10),
            Duration::from_millis(1000)
        ),
        // prevote
        Timeout::new(
            Duration::from_millis(100),
            Duration::from_millis(10),
            Duration::from_millis(500)
        ),
        // precommit
        Timeout::new(
            Duration::from_millis(100),
            Duration::from_millis(10),
            Duration::from_millis(500)
        )
    );
}

const CHANNEL_SIZE: usize = 10;
const HEIGHT_0: BlockNumber = BlockNumber(0);
const HEIGHT_1: BlockNumber = BlockNumber(1);
const HEIGHT_2: BlockNumber = BlockNumber(2);
const ROUND_0: Round = 0;
const ROUND_1: Round = 1;
const ROUND_2: Round = 2;
const SYNC_RETRY_INTERVAL: Duration = Duration::from_millis(100);

#[fixture]
fn consensus_config() -> ConsensusConfig {
    ConsensusConfig::from_parts(
        ConsensusDynamicConfig {
            validator_id: *VALIDATOR_ID,
            timeouts: TIMEOUTS.clone(),
            sync_retry_interval: SYNC_RETRY_INTERVAL,
            future_msg_limit: FutureMsgLimitsConfig::default(),
        },
        ConsensusStaticConfig {
            storage_config: StorageConfig::default(),
            startup_delay: Duration::ZERO,
        },
    )
}

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
            block_sender.send(ProposalCommitment(block_hash)).unwrap();
            block_receiver
        })
        .times(times);
}

fn assert_decision(res: RunHeightRes, id: Felt, round: Round) {
    match res {
        RunHeightRes::Decision(decision) => {
            assert_eq!(decision.block, ProposalCommitment(id));
            assert_eq!(decision.precommits[0].round, round);
        }
        _ => panic!("Expected decision"),
    }
}

#[rstest]
#[tokio::test]
async fn manager_multiple_heights_unordered(consensus_config: ConsensusConfig) {
    let TestSubscriberChannels { mock_network, subscriber_channels } =
        mock_register_broadcast_topic().unwrap();
    let mut sender = mock_network.broadcasted_messages_sender;

    let (mut proposal_receiver_sender, mut proposal_receiver_receiver) =
        mpsc::channel(CHANNEL_SIZE);

    // Send messages for height 2 followed by those for height 1.
    send_proposal(
        &mut proposal_receiver_sender,
        vec![TestProposalPart::BlockInfo(block_info(HEIGHT_2, ROUND_0, *PROPOSER_ID))],
    )
    .await;
    send(&mut sender, prevote(Some(Felt::TWO), HEIGHT_2, ROUND_0, *PROPOSER_ID)).await;
    send(&mut sender, precommit(Some(Felt::TWO), HEIGHT_2, ROUND_0, *PROPOSER_ID)).await;

    send_proposal(
        &mut proposal_receiver_sender,
        vec![TestProposalPart::BlockInfo(block_info(HEIGHT_1, ROUND_0, *PROPOSER_ID))],
    )
    .await;
    send(&mut sender, prevote(Some(Felt::ONE), HEIGHT_1, ROUND_0, *PROPOSER_ID)).await;
    send(&mut sender, precommit(Some(Felt::ONE), HEIGHT_1, ROUND_0, *PROPOSER_ID)).await;

    let mut context = MockTestContext::new();
    // Run the manager for height 1.
    context.expect_try_sync().returning(|_| false);
    expect_validate_proposal(&mut context, Felt::ONE, 1);
    context.expect_validators().returning(move |_| Ok(vec![*PROPOSER_ID, *VALIDATOR_ID]));
    context.expect_proposer().returning(move |_, _| Ok(*PROPOSER_ID));
    context.expect_virtual_proposer().returning(move |_, _| Ok(*PROPOSER_ID));
    context.expect_set_height_and_round().returning(move |_, _| Ok(()));
    context.expect_broadcast().returning(move |_| Ok(()));
    context
        .expect_decision_reached()
        .withf(move |_, c| *c == ProposalCommitment(Felt::ONE))
        .return_once(move |_, _| Ok(()));
    context
        .expect_decision_reached()
        .withf(move |_, c| *c == ProposalCommitment(Felt::TWO))
        .return_once(move |_, _| Ok(()));

    let mut manager = MultiHeightManager::new(
        consensus_config,
        QuorumType::Byzantine,
        Arc::new(Mutex::new(NoOpHeightVotedStorage)),
    )
    .await;
    let mut subscriber_channels = subscriber_channels.into();
    let decision = manager
        .run_height(
            &mut context,
            HEIGHT_1,
            &mut subscriber_channels,
            &mut proposal_receiver_receiver,
        )
        .await
        .unwrap();
    assert_decision(decision, Felt::ONE, ROUND_0);

    // Run the manager for height 2.
    expect_validate_proposal(&mut context, Felt::TWO, 1);
    let decision = manager
        .run_height(
            &mut context,
            HEIGHT_2,
            &mut subscriber_channels,
            &mut proposal_receiver_receiver,
        )
        .await
        .unwrap();
    assert_decision(decision, Felt::TWO, ROUND_0);
}

#[rstest]
#[tokio::test]
async fn run_consensus_sync(consensus_config: ConsensusConfig) {
    // Set expectations.
    let mut context = MockTestContext::new();
    let (decision_tx, decision_rx) = oneshot::channel();

    let (mut proposal_receiver_sender, proposal_receiver_receiver) = mpsc::channel(CHANNEL_SIZE);

    expect_validate_proposal(&mut context, Felt::TWO, 1);
    context.expect_validators().returning(move |_| Ok(vec![*PROPOSER_ID, *VALIDATOR_ID]));
    context.expect_proposer().returning(move |_, _| Ok(*PROPOSER_ID));
    context.expect_virtual_proposer().returning(move |_, _| Ok(*PROPOSER_ID));
    context.expect_set_height_and_round().returning(move |_, _| Ok(()));
    context.expect_broadcast().returning(move |_| Ok(()));
    context
        .expect_decision_reached()
        .withf(move |h, c| *c == ProposalCommitment(Felt::TWO) && *h == HEIGHT_2)
        .return_once(move |_, _| {
            decision_tx.send(()).unwrap();
            Ok(())
        });
    context.expect_try_sync().withf(move |height| *height == HEIGHT_1).times(1).returning(|_| true);
    context.expect_try_sync().returning(|_| false);

    // Send messages for height 2.
    send_proposal(
        &mut proposal_receiver_sender,
        vec![TestProposalPart::BlockInfo(block_info(HEIGHT_2, ROUND_0, *PROPOSER_ID))],
    )
    .await;
    let TestSubscriberChannels { mock_network, subscriber_channels } =
        mock_register_broadcast_topic().unwrap();
    let mut network_sender = mock_network.broadcasted_messages_sender;
    send(&mut network_sender, prevote(Some(Felt::TWO), HEIGHT_2, ROUND_0, *PROPOSER_ID)).await;
    send(&mut network_sender, precommit(Some(Felt::TWO), HEIGHT_2, ROUND_0, *PROPOSER_ID)).await;
    let run_consensus_args = RunConsensusArguments {
        consensus_config,
        start_active_height: HEIGHT_1,
        quorum_type: QuorumType::Byzantine,
        config_manager_client: None,
        last_voted_height_storage: Arc::new(Mutex::new(NoOpHeightVotedStorage)),
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

#[rstest]
#[tokio::test]
async fn test_timeouts(consensus_config: ConsensusConfig) {
    let TestSubscriberChannels { mock_network, subscriber_channels } =
        mock_register_broadcast_topic().unwrap();
    let mut sender = mock_network.broadcasted_messages_sender;

    let (mut proposal_receiver_sender, mut proposal_receiver_receiver) =
        mpsc::channel(CHANNEL_SIZE);

    send_proposal(
        &mut proposal_receiver_sender,
        vec![TestProposalPart::BlockInfo(block_info(HEIGHT_1, ROUND_0, *PROPOSER_ID))],
    )
    .await;
    send(&mut sender, prevote(None, HEIGHT_1, ROUND_0, *VALIDATOR_ID_2)).await;
    send(&mut sender, prevote(None, HEIGHT_1, ROUND_0, *VALIDATOR_ID_3)).await;
    send(&mut sender, precommit(None, HEIGHT_1, ROUND_0, *VALIDATOR_ID_2)).await;
    send(&mut sender, precommit(None, HEIGHT_1, ROUND_0, *VALIDATOR_ID_3)).await;

    let mut context = MockTestContext::new();
    context.expect_virtual_proposer().returning(move |_, _| Ok(*PROPOSER_ID));
    context.expect_set_height_and_round().returning(move |_, _| Ok(()));
    expect_validate_proposal(&mut context, Felt::ONE, 2);
    context.expect_validators().returning(move |_| {
        Ok(vec![*PROPOSER_ID, *VALIDATOR_ID, *VALIDATOR_ID_2, *VALIDATOR_ID_3])
    });
    context.expect_proposer().returning(move |_, _| Ok(*PROPOSER_ID));
    context.expect_try_sync().returning(|_| false);

    let (timeout_send, timeout_receive) = oneshot::channel();
    // Node handled Timeout events and responded with NIL vote.
    context
        .expect_broadcast()
        .times(1)
        .withf(move |msg: &Vote| msg == &prevote(None, HEIGHT_1, ROUND_1, *VALIDATOR_ID))
        .return_once(move |_| {
            timeout_send.send(()).unwrap();
            Ok(())
        });
    context.expect_broadcast().returning(move |_| Ok(()));
    context
        .expect_decision_reached()
        .withf(move |_, c| *c == ProposalCommitment(Felt::ONE))
        .return_once(move |_, _| Ok(()));

    // Ensure our validator id matches the expectation in the broadcast assertion.
    let mut manager = MultiHeightManager::new(
        consensus_config,
        QuorumType::Byzantine,
        Arc::new(Mutex::new(NoOpHeightVotedStorage)),
    )
    .await;
    let manager_handle = tokio::spawn(async move {
        let decision = manager
            .run_height(
                &mut context,
                HEIGHT_1,
                &mut subscriber_channels.into(),
                &mut proposal_receiver_receiver,
            )
            .await
            .unwrap();
        assert_decision(decision, Felt::ONE, ROUND_1);
    });

    // Wait for the timeout to be triggered.
    timeout_receive.await.unwrap();
    // Show that after the timeout is triggered we can still precommit in favor of the block and
    // reach a decision.
    send_proposal(
        &mut proposal_receiver_sender,
        vec![TestProposalPart::BlockInfo(block_info(HEIGHT_1, ROUND_1, *PROPOSER_ID))],
    )
    .await;
    send(&mut sender, prevote(Some(Felt::ONE), HEIGHT_1, ROUND_1, *PROPOSER_ID)).await;
    send(&mut sender, prevote(Some(Felt::ONE), HEIGHT_1, ROUND_1, *VALIDATOR_ID_2)).await;
    send(&mut sender, prevote(Some(Felt::ONE), HEIGHT_1, ROUND_1, *VALIDATOR_ID_3)).await;
    send(&mut sender, precommit(Some(Felt::ONE), HEIGHT_1, ROUND_1, *PROPOSER_ID)).await;
    send(&mut sender, precommit(Some(Felt::ONE), HEIGHT_1, ROUND_1, *VALIDATOR_ID_2)).await;

    manager_handle.await.unwrap();
}

#[rstest]
#[tokio::test]
async fn timely_message_handling(consensus_config: ConsensusConfig) {
    // TODO(matan): Make run_height more generic so don't need mock network?
    // Check that, even when sync is immediately ready, consensus still handles queued messages.
    let mut context = MockTestContext::new();
    context.expect_try_sync().returning(|_| true);
    context.expect_virtual_proposer().returning(move |_, _| Ok(*PROPOSER_ID));

    // Send messages
    let (mut proposal_receiver_sender, mut proposal_receiver_receiver) = mpsc::channel(0);
    let (mut content_sender, content_receiver) = mpsc::channel(0);
    content_sender
        .try_send(TestProposalPart::BlockInfo(block_info(HEIGHT_1, ROUND_0, *PROPOSER_ID)))
        .unwrap();
    proposal_receiver_sender.try_send(content_receiver).unwrap();

    // Fill up the sender.
    let TestSubscriberChannels { mock_network, subscriber_channels } =
        mock_register_broadcast_topic().unwrap();
    let mut subscriber_channels = subscriber_channels.into();
    let mut vote_sender = mock_network.broadcasted_messages_sender;
    let metadata = BroadcastedMessageMetadata::get_test_instance(&mut get_rng());
    let vote = prevote(Some(Felt::TWO), HEIGHT_1, ROUND_0, *PROPOSER_ID);
    // Fill up the buffer.
    while vote_sender.send((vote.clone(), metadata.clone())).now_or_never().is_some() {}

    let mut manager = MultiHeightManager::new(
        consensus_config,
        QuorumType::Byzantine,
        Arc::new(Mutex::new(NoOpHeightVotedStorage)),
    )
    .await;
    let res = manager
        .run_height(
            &mut context,
            HEIGHT_1,
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

#[rstest]
#[tokio::test]
async fn future_height_limit_caching_and_dropping(mut consensus_config: ConsensusConfig) {
    // Use very low limit - only cache 1 height ahead with round 0.
    consensus_config.dynamic_config.future_msg_limit = FutureMsgLimitsConfig {
        future_height_limit: 1,
        future_round_limit: 0,
        future_height_round_limit: 0,
    };

    let TestSubscriberChannels { mock_network, subscriber_channels } =
        mock_register_broadcast_topic().unwrap();
    let mut sender = mock_network.broadcasted_messages_sender;

    let (mut proposal_receiver_sender, mut proposal_receiver_receiver) =
        mpsc::channel(CHANNEL_SIZE);

    // Send proposal and votes for height 2 (should be dropped when processing height 0).
    send_proposal(
        &mut proposal_receiver_sender,
        vec![TestProposalPart::BlockInfo(block_info(HEIGHT_2, ROUND_0, *PROPOSER_ID))],
    )
    .await;
    send(&mut sender, prevote(Some(Felt::TWO), HEIGHT_2, ROUND_0, *PROPOSER_ID)).await;
    send(&mut sender, precommit(Some(Felt::TWO), HEIGHT_2, ROUND_0, *PROPOSER_ID)).await;

    // Send proposal and votes for height 1 (should be cached when processing height 0).
    send_proposal(
        &mut proposal_receiver_sender,
        vec![TestProposalPart::BlockInfo(block_info(HEIGHT_1, ROUND_0, *PROPOSER_ID))],
    )
    .await;
    send(&mut sender, prevote(Some(Felt::ONE), HEIGHT_1, ROUND_0, *PROPOSER_ID)).await;
    send(&mut sender, precommit(Some(Felt::ONE), HEIGHT_1, ROUND_0, *PROPOSER_ID)).await;

    // Send proposal and votes for height 0 (current height - needed to reach consensus).
    send_proposal(
        &mut proposal_receiver_sender,
        vec![TestProposalPart::BlockInfo(block_info(HEIGHT_0, ROUND_0, *PROPOSER_ID))],
    )
    .await;
    send(&mut sender, prevote(Some(Felt::ZERO), HEIGHT_0, ROUND_0, *PROPOSER_ID)).await;
    send(&mut sender, precommit(Some(Felt::ZERO), HEIGHT_0, ROUND_0, *PROPOSER_ID)).await;

    let mut context = MockTestContext::new();
    context.expect_try_sync().returning(|_| false);
    expect_validate_proposal(&mut context, Felt::ZERO, 1); // Height 0 validation
    expect_validate_proposal(&mut context, Felt::ONE, 1); // Height 1 validation
    context.expect_validators().returning(move |_| Ok(vec![*PROPOSER_ID, *VALIDATOR_ID]));
    context.expect_proposer().returning(move |_, _| Ok(*PROPOSER_ID));
    context.expect_virtual_proposer().returning(move |_, _| Ok(*PROPOSER_ID));
    context.expect_set_height_and_round().returning(move |_, _| Ok(()));
    // Set up coordination to detect when node votes Nil for height 2 (indicating proposal was
    // dropped, so the node didn't received the proposal and votes Nil).
    let (height2_nil_vote_trigger, height2_nil_vote_wait) = oneshot::channel();
    context
        .expect_broadcast()
        .withf(move |vote: &Vote| vote.height == HEIGHT_2 && vote.proposal_commitment.is_none())
        .times(1)
        .return_once(move |_| {
            height2_nil_vote_trigger.send(()).unwrap();
            Ok(())
        });
    // Handle all other broadcasts normally.
    context.expect_broadcast().returning(move |_| Ok(()));
    context
        .expect_decision_reached()
        .withf(move |_, c| *c == ProposalCommitment(Felt::ZERO))
        .return_once(move |_, _| Ok(()));
    context
        .expect_decision_reached()
        .withf(move |_, c| *c == ProposalCommitment(Felt::ONE))
        .return_once(move |_, _| Ok(()));

    let mut manager = MultiHeightManager::new(
        consensus_config,
        QuorumType::Byzantine,
        Arc::new(Mutex::new(NoOpHeightVotedStorage)),
    )
    .await;
    let mut subscriber_channels = subscriber_channels.into();

    // Run height 0 - should drop height 2 messages, cache height 1 messages, and reach consensus.
    let decision = manager
        .run_height(
            &mut context,
            HEIGHT_0,
            &mut subscriber_channels,
            &mut proposal_receiver_receiver,
        )
        .await
        .unwrap();
    assert_decision(decision, Felt::ZERO, ROUND_0);

    // Run height 1 - should succeed using cached proposal.
    let decision = manager
        .run_height(
            &mut context,
            HEIGHT_1,
            &mut subscriber_channels,
            &mut proposal_receiver_receiver,
        )
        .await
        .unwrap();
    assert_decision(decision, Felt::ONE, ROUND_0);

    // Run height 2 in background - shouldn't reach consensus because proposal was dropped.
    let manager_handle = tokio::spawn(async move {
        manager
            .run_height(
                &mut context,
                HEIGHT_2,
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

#[rstest]
#[tokio::test]
async fn current_height_round_limit_caching_and_dropping(mut consensus_config: ConsensusConfig) {
    consensus_config.dynamic_config.future_msg_limit = FutureMsgLimitsConfig {
        future_height_limit: 10,
        future_round_limit: 0, // Accept only current round (current_round + 0).
        future_height_round_limit: 1,
    };

    let TestSubscriberChannels { mock_network, subscriber_channels } =
        mock_register_broadcast_topic().unwrap();
    let mut sender = mock_network.broadcasted_messages_sender;

    let (mut proposal_receiver_sender, mut proposal_receiver_receiver) =
        mpsc::channel(CHANNEL_SIZE);

    // Send proposals for rounds 0 and 1, proposal for round 1 should be dropped.
    send_proposal(
        &mut proposal_receiver_sender,
        vec![TestProposalPart::BlockInfo(block_info(HEIGHT_1, ROUND_0, *PROPOSER_ID))],
    )
    .await;
    send_proposal(
        &mut proposal_receiver_sender,
        vec![TestProposalPart::BlockInfo(block_info(HEIGHT_1, ROUND_1, *PROPOSER_ID))],
    )
    .await;

    // Send votes for round 1. These should be dropped because when state machine is in round 0,
    // round 1 > current_round(0) + future_round_limit(0).
    send(&mut sender, prevote(Some(Felt::ONE), HEIGHT_1, ROUND_1, *PROPOSER_ID)).await;
    send(&mut sender, prevote(Some(Felt::ONE), HEIGHT_1, ROUND_1, *VALIDATOR_ID_2)).await;
    send(&mut sender, precommit(Some(Felt::ONE), HEIGHT_1, ROUND_1, *PROPOSER_ID)).await;
    send(&mut sender, precommit(Some(Felt::ONE), HEIGHT_1, ROUND_1, *VALIDATOR_ID_2)).await;

    // Send Nil votes for round 0 (current round).
    send(&mut sender, prevote(None, HEIGHT_1, ROUND_0, *VALIDATOR_ID_2)).await;
    send(&mut sender, prevote(None, HEIGHT_1, ROUND_0, *PROPOSER_ID)).await;
    send(&mut sender, precommit(None, HEIGHT_1, ROUND_0, *VALIDATOR_ID_2)).await;
    send(&mut sender, precommit(None, HEIGHT_1, ROUND_0, *PROPOSER_ID)).await;

    let mut context = MockTestContext::new();
    context.expect_try_sync().returning(|_| false);
    // Will be called twice for round 0 and 2 (will send the proposal when advancing to round 2).
    expect_validate_proposal(&mut context, Felt::ONE, 2);
    context
        .expect_validators()
        .returning(move |_| Ok(vec![*PROPOSER_ID, *VALIDATOR_ID, *VALIDATOR_ID_2]));
    context.expect_proposer().returning(move |_, _| Ok(*PROPOSER_ID));
    context.expect_virtual_proposer().returning(move |_, _| Ok(*PROPOSER_ID));
    context.expect_broadcast().returning(move |_| Ok(()));

    // Set up coordination for round advancement.
    let (round1_trigger, round1_wait) = oneshot::channel();
    let (round2_trigger, round2_wait) = oneshot::channel();

    context
        .expect_set_height_and_round()
        .withf(|height, round| *height == HEIGHT_1 && *round == ROUND_1)
        .times(1)
        .return_once(|_, _| {
            round1_trigger.send(()).unwrap();
            Ok(())
        });
    context
        .expect_set_height_and_round()
        .withf(|height, round| *height == HEIGHT_1 && *round == ROUND_2)
        .times(1)
        .return_once(|_, _| {
            round2_trigger.send(()).unwrap();
            Ok(())
        });
    // Handle all other set_height_and_round calls normally.
    context.expect_set_height_and_round().returning(move |_, _| Ok(()));
    context
        .expect_decision_reached()
        .withf(move |_, c| *c == ProposalCommitment(Felt::ONE))
        .return_once(move |_, _| Ok(()));

    let mut manager = MultiHeightManager::new(
        consensus_config,
        QuorumType::Byzantine,
        Arc::new(Mutex::new(NoOpHeightVotedStorage)),
    )
    .await;
    let mut subscriber_channels = subscriber_channels.into();

    // Spawn tasks to send messages when rounds advance.
    let mut sender_clone1 = sender.clone();
    tokio::spawn(async move {
        round1_wait.await.unwrap();
        // Send Nil votes from other nodes for round 1.
        send(&mut sender_clone1, prevote(None, HEIGHT_1, ROUND_1, *VALIDATOR_ID_2)).await;
        send(&mut sender_clone1, prevote(None, HEIGHT_1, ROUND_1, *PROPOSER_ID)).await;
        send(&mut sender_clone1, precommit(None, HEIGHT_1, ROUND_1, *VALIDATOR_ID_2)).await;
        send(&mut sender_clone1, precommit(None, HEIGHT_1, ROUND_1, *PROPOSER_ID)).await;
    });

    let mut sender_clone2 = sender.clone();
    let mut proposal_sender_clone = proposal_receiver_sender.clone();
    tokio::spawn(async move {
        round2_wait.await.unwrap();
        // Send proposal for round 2.
        send_proposal(
            &mut proposal_sender_clone,
            vec![TestProposalPart::BlockInfo(block_info(HEIGHT_1, ROUND_2, *PROPOSER_ID))],
        )
        .await;
        // Send votes for round 2.
        send(&mut sender_clone2, prevote(Some(Felt::ONE), HEIGHT_1, ROUND_2, *PROPOSER_ID)).await;
        send(&mut sender_clone2, prevote(Some(Felt::ONE), HEIGHT_1, ROUND_2, *VALIDATOR_ID_2))
            .await;
        send(&mut sender_clone2, precommit(Some(Felt::ONE), HEIGHT_1, ROUND_2, *PROPOSER_ID)).await;
        send(&mut sender_clone2, precommit(Some(Felt::ONE), HEIGHT_1, ROUND_2, *VALIDATOR_ID_2))
            .await;
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
            HEIGHT_1,
            &mut subscriber_channels,
            &mut proposal_receiver_receiver,
        )
        .await
        .unwrap();
    assert_decision(decision, Felt::ONE, ROUND_2);
}

#[rstest]
#[tokio::test]
async fn run_consensus_dynamic_client_updates_validator_between_heights(
    consensus_config: ConsensusConfig,
) {
    let TestSubscriberChannels { mock_network, subscriber_channels } =
        mock_register_broadcast_topic().unwrap();
    // Keep a handle to the vote sender so the paired receiver stays alive.
    let _vote_sender = mock_network.broadcasted_messages_sender;
    let (_proposal_receiver_sender, proposal_receiver_receiver) = mpsc::channel(CHANNEL_SIZE);

    // Context with expectations: H1 we are the validator, learn height via sync; at H2 we are the
    // proposer.
    let mut context = MockTestContext::new();
    context.expect_set_height_and_round().returning(move |_, _| Ok(()));
    context.expect_validators().returning(move |h: BlockNumber| {
        Ok(if h == HEIGHT_1 { vec![*VALIDATOR_ID] } else { vec![*PROPOSER_ID] })
    });
    context.expect_proposer().returning(move |h: BlockNumber, _| {
        Ok(if h == HEIGHT_1 { *VALIDATOR_ID } else { *PROPOSER_ID })
    });
    context.expect_virtual_proposer().returning(move |h: BlockNumber, _| {
        Ok(if h == HEIGHT_1 { *VALIDATOR_ID } else { *PROPOSER_ID })
    });
    context.expect_try_sync().withf(move |h| *h == HEIGHT_1).times(1).returning(|_| true);
    context.expect_try_sync().returning(|_| false);
    context.expect_broadcast().returning(move |_| Ok(()));

    // In this test, build_proposal should be called only when the dynamic config returns that we
    // are the proposer, which happens at H2.
    context
        .expect_build_proposal()
        .withf(move |init, _| init.height == HEIGHT_2 && init.proposer == *PROPOSER_ID)
        .returning(move |_, _| {
            let (sender, receiver) = oneshot::channel();
            sender.send(ProposalCommitment(Felt::TWO)).unwrap();
            Ok(receiver)
        })
        .times(1);
    // Expect a decision at height 2.
    let (decision_tx, decision_rx) = oneshot::channel();
    context
        .expect_decision_reached()
        .withf(move |h, _| *h == HEIGHT_2)
        .return_once(move |_, _| {
            let _ = decision_tx.send(());
            Ok(())
        })
        .times(1);

    // Dynamic client mock: H1 -> VALIDATOR_ID, H2 -> PROPOSER_ID (order is important)
    let mut mock_client = MockConfigManagerClient::new();
    let validator_config = consensus_config.dynamic_config.clone();
    let proposer_config =
        ConsensusDynamicConfig { validator_id: *PROPOSER_ID, ..validator_config.clone() };
    mock_client.expect_get_consensus_dynamic_config().times(1).return_const(Ok(validator_config));
    mock_client.expect_get_consensus_dynamic_config().times(1).return_const(Ok(proposer_config));

    let run_consensus_args = RunConsensusArguments {
        start_active_height: HEIGHT_1,
        consensus_config,
        quorum_type: QuorumType::Byzantine,
        config_manager_client: Some(Arc::new(mock_client)),
        last_voted_height_storage: Arc::new(Mutex::new(NoOpHeightVotedStorage)),
    };

    // Spawn consensus and wait for a decision at height 2.
    tokio::spawn(async move {
        run_consensus(
            run_consensus_args,
            context,
            subscriber_channels.into(),
            proposal_receiver_receiver,
        )
        .await
    });
    decision_rx.await.unwrap();
}

#[rstest]
#[tokio::test]
async fn manager_successfully_syncs_when_higher_than_last_voted_height(
    consensus_config: ConsensusConfig,
) {
    const LAST_VOTED_HEIGHT: BlockNumber = HEIGHT_1;
    const CURRENT_HEIGHT: BlockNumber = BlockNumber(LAST_VOTED_HEIGHT.0 + 1);

    let TestSubscriberChannels {
        #[allow(unused)]
        // We need this to stay alive so that the network wont be automatically closed.
        mock_network,
        subscriber_channels,
    } = mock_register_broadcast_topic().unwrap();

    let (_proposal_receiver_sender, mut proposal_receiver_receiver) = mpsc::channel(CHANNEL_SIZE);

    let mut mock_height_voted_storage = MockHeightVotedStorageTrait::new();
    mock_height_voted_storage
        .expect_get_prev_voted_height()
        .returning(|| Ok(Some(LAST_VOTED_HEIGHT)));

    let mut context = MockTestContext::new();
    context.expect_try_sync().with(eq(CURRENT_HEIGHT)).times(1).returning(|_| true);

    let mut manager = MultiHeightManager::new(
        consensus_config,
        QuorumType::Byzantine,
        Arc::new(Mutex::new(mock_height_voted_storage)),
    )
    .await;
    let mut subscriber_channels = subscriber_channels.into();
    let decision = manager
        .run_height(
            &mut context,
            CURRENT_HEIGHT,
            &mut subscriber_channels,
            &mut proposal_receiver_receiver,
        )
        .await
        .unwrap();

    assert_eq!(decision, RunHeightRes::Sync);
}

#[rstest]
#[tokio::test]
async fn manager_runs_normally_when_height_is_greater_than_last_voted_height(
    consensus_config: ConsensusConfig,
) {
    const LAST_VOTED_HEIGHT: BlockNumber = HEIGHT_1;
    const CURRENT_HEIGHT: BlockNumber = BlockNumber(LAST_VOTED_HEIGHT.0 + 1);

    let TestSubscriberChannels { mock_network, subscriber_channels } =
        mock_register_broadcast_topic().unwrap();
    let mut sender = mock_network.broadcasted_messages_sender;

    let (mut proposal_receiver_sender, mut proposal_receiver_receiver) =
        mpsc::channel(CHANNEL_SIZE);

    let mut mock_height_voted_storage = MockHeightVotedStorageTrait::new();
    mock_height_voted_storage
        .expect_get_prev_voted_height()
        .returning(|| Ok(Some(LAST_VOTED_HEIGHT)));
    // After voting on this proposal, SingleHeightConsensus will set the last voted height in
    // storage. This is out of scope for a unit test but since there is no dependency injection we
    // must set this expectation.
    mock_height_voted_storage
        .expect_set_prev_voted_height()
        .with(eq(CURRENT_HEIGHT))
        .returning(|_| Ok(()));

    // Send a proposal for the height we already voted on:
    send_proposal(
        &mut proposal_receiver_sender,
        vec![TestProposalPart::BlockInfo(block_info(CURRENT_HEIGHT, ROUND_0, *PROPOSER_ID))],
    )
    .await;
    send(&mut sender, prevote(Some(Felt::ONE), CURRENT_HEIGHT, ROUND_0, *PROPOSER_ID)).await;
    send(&mut sender, precommit(Some(Felt::ONE), CURRENT_HEIGHT, ROUND_0, *PROPOSER_ID)).await;

    let mut context = MockTestContext::new();
    // Sync will never succeed so we will proceed to run consensus (during which try_sync is called
    // periodically regardless of last voted height functionality).
    context.expect_try_sync().with(eq(CURRENT_HEIGHT)).returning(|_| false);
    expect_validate_proposal(&mut context, Felt::ONE, 1);
    context.expect_validators().returning(move |_| Ok(vec![*PROPOSER_ID, *VALIDATOR_ID]));
    context.expect_proposer().returning(move |_, _| Ok(*PROPOSER_ID));
    context.expect_virtual_proposer().returning(move |_, _| Ok(*PROPOSER_ID));
    context.expect_set_height_and_round().returning(move |_, _| Ok(()));
    context.expect_broadcast().returning(move |_| Ok(()));
    context
        .expect_decision_reached()
        .withf(move |_, c| *c == ProposalCommitment(Felt::ONE))
        .return_once(move |_, _| Ok(()));

    let mut manager = MultiHeightManager::new(
        consensus_config,
        QuorumType::Byzantine,
        Arc::new(Mutex::new(mock_height_voted_storage)),
    )
    .await;
    let mut subscriber_channels = subscriber_channels.into();
    let decision = manager
        .run_height(
            &mut context,
            CURRENT_HEIGHT,
            &mut subscriber_channels,
            &mut proposal_receiver_receiver,
        )
        .await
        .unwrap();

    assert_decision(decision, Felt::ONE, ROUND_0);
}

#[rstest]
#[tokio::test]
async fn manager_waits_until_height_passes_last_voted_height(consensus_config: ConsensusConfig) {
    const LAST_VOTED_HEIGHT: BlockNumber = HEIGHT_1;

    let TestSubscriberChannels { mock_network, subscriber_channels } =
        mock_register_broadcast_topic().unwrap();
    let mut sender = mock_network.broadcasted_messages_sender;

    let (mut proposal_receiver_sender, mut proposal_receiver_receiver) =
        mpsc::channel(CHANNEL_SIZE);

    let mut mock_height_voted_storage = MockHeightVotedStorageTrait::new();
    mock_height_voted_storage
        .expect_get_prev_voted_height()
        .returning(|| Ok(Some(LAST_VOTED_HEIGHT)));

    // Send a proposal for the height we already voted on:
    send_proposal(
        &mut proposal_receiver_sender,
        vec![TestProposalPart::BlockInfo(block_info(LAST_VOTED_HEIGHT, ROUND_0, *PROPOSER_ID))],
    )
    .await;
    send(&mut sender, prevote(Some(Felt::ONE), LAST_VOTED_HEIGHT, ROUND_0, *PROPOSER_ID)).await;
    send(&mut sender, precommit(Some(Felt::ONE), LAST_VOTED_HEIGHT, ROUND_0, *PROPOSER_ID)).await;

    let mut context = MockTestContext::new();
    // At the last voted height we expect the manager to halt until it can get the last voted height
    // from storage. We wait 3 retries to make sure it retries.
    context.expect_try_sync().with(eq(LAST_VOTED_HEIGHT)).times(3).returning(|_| false);
    context.expect_try_sync().with(eq(LAST_VOTED_HEIGHT)).times(1).returning(|_| true);
    context.expect_virtual_proposer().returning(move |_, _| Ok(*PROPOSER_ID));

    let mut manager = MultiHeightManager::new(
        consensus_config,
        QuorumType::Byzantine,
        Arc::new(Mutex::new(mock_height_voted_storage)),
    )
    .await;
    let mut subscriber_channels = subscriber_channels.into();
    let decision = manager
        .run_height(
            &mut context,
            LAST_VOTED_HEIGHT,
            &mut subscriber_channels,
            &mut proposal_receiver_receiver,
        )
        .await
        .unwrap();

    assert_eq!(decision, RunHeightRes::Sync);
}

#[rstest]
#[tokio::test]
async fn writes_voted_height_to_storage(consensus_config: ConsensusConfig) {
    const HEIGHT: BlockNumber = BlockNumber(123);
    const LAST_VOTED_HEIGHT: BlockNumber = BlockNumber(HEIGHT.0 - 1);
    let block_id = ProposalCommitment(Felt::ONE);

    let mut mock_height_voted_storage = MockHeightVotedStorageTrait::new();
    mock_height_voted_storage
        .expect_get_prev_voted_height()
        .returning(|| Ok(Some(LAST_VOTED_HEIGHT)));

    let mut storage_before_broadcast_sequence = Sequence::new();

    let TestSubscriberChannels { mock_network, subscriber_channels } =
        mock_register_broadcast_topic().unwrap();
    let sender = mock_network.broadcasted_messages_sender;

    let (mut proposal_receiver_sender, mut proposal_receiver_receiver) =
        mpsc::channel(CHANNEL_SIZE);

    let mut context = MockTestContext::new();
    context.expect_proposer().returning(move |_, _| Ok(*PROPOSER_ID));
    context.expect_virtual_proposer().returning(move |_, _| Ok(*PROPOSER_ID));
    context.expect_validators().returning(move |_| {
        Ok(vec![*PROPOSER_ID, *VALIDATOR_ID, *VALIDATOR_ID_2, *VALIDATOR_ID_3])
    });
    context.expect_set_height_and_round().returning(move |_, _| Ok(()));
    context.expect_try_sync().returning(|_| false);

    // Set up storage expectation for prevote - must happen before broadcast
    mock_height_voted_storage
        .expect_set_prev_voted_height()
        .with(mockall::predicate::eq(HEIGHT))
        .times(1)
        .in_sequence(&mut storage_before_broadcast_sequence)
        .returning(move |_| Ok(()));

    // Set up broadcast expectation for prevote - must happen after storage write
    // Use a channel to signal when prevote is broadcast so we can send other votes
    let (prevote_tx, prevote_rx) = oneshot::channel();
    let mut prevote_tx = Some(prevote_tx);
    context
        .expect_broadcast()
        .times(1)
        .withf(move |msg: &Vote| msg == &prevote(Some(block_id.0), HEIGHT, ROUND_0, *VALIDATOR_ID))
        .in_sequence(&mut storage_before_broadcast_sequence)
        .returning(move |_| {
            if let Some(tx) = prevote_tx.take() {
                let _ = tx.send(());
            }
            Ok(())
        });

    // Set up storage expectation for precommit - must happen before broadcast
    mock_height_voted_storage
        .expect_set_prev_voted_height()
        .with(mockall::predicate::eq(HEIGHT))
        .times(1)
        .in_sequence(&mut storage_before_broadcast_sequence)
        .returning(move |_| Ok(()));

    // Set up broadcast expectation for precommit - must happen after storage write
    // Use a channel to signal when precommit is broadcast
    let (precommit_tx, precommit_rx) = oneshot::channel();
    let mut precommit_tx_shared = Some(precommit_tx);
    context
        .expect_broadcast()
        .times(1)
        .withf(move |msg: &Vote| {
            msg == &precommit(Some(block_id.0), HEIGHT, ROUND_0, *VALIDATOR_ID)
        })
        .in_sequence(&mut storage_before_broadcast_sequence)
        .returning(move |_| {
            if let Some(tx) = precommit_tx_shared.take() {
                let _ = tx.send(());
            }
            Ok(())
        });

    // Set up validation expectation
    context.expect_validate_proposal().returning(move |_, _, _| {
        let (block_sender, block_receiver) = oneshot::channel();
        block_sender.send(block_id).unwrap();
        block_receiver
    });

    // Send proposal first
    send_proposal(
        &mut proposal_receiver_sender,
        vec![TestProposalPart::BlockInfo(block_info(HEIGHT, ROUND_0, *PROPOSER_ID))],
    )
    .await;

    context
        .expect_decision_reached()
        .withf(move |_, c| *c == block_id)
        .return_once(move |_, _| Ok(()));

    let mut manager = MultiHeightManager::new(
        consensus_config,
        QuorumType::Byzantine,
        Arc::new(Mutex::new(mock_height_voted_storage)),
    )
    .await;
    let mut subscriber_channels = subscriber_channels.into();

    // Spawn a task to send votes after validator votes
    let mut sender_clone = sender.clone();
    let block_id_for_votes = block_id;
    tokio::spawn(async move {
        // Wait for validator to validate proposal and broadcast prevote
        prevote_rx.await.unwrap();

        // Now send other prevotes to reach quorum
        send(&mut sender_clone, prevote(Some(block_id_for_votes.0), HEIGHT, ROUND_0, *PROPOSER_ID))
            .await;
        send(
            &mut sender_clone,
            prevote(Some(block_id_for_votes.0), HEIGHT, ROUND_0, *VALIDATOR_ID_2),
        )
        .await;
        send(
            &mut sender_clone,
            prevote(Some(block_id_for_votes.0), HEIGHT, ROUND_0, *VALIDATOR_ID_3),
        )
        .await;

        // Wait for validator to broadcast precommit after reaching prevote quorum
        precommit_rx.await.unwrap();

        // Now send other precommits to reach decision
        send(
            &mut sender_clone,
            precommit(Some(block_id_for_votes.0), HEIGHT, ROUND_0, *PROPOSER_ID),
        )
        .await;
        send(
            &mut sender_clone,
            precommit(Some(block_id_for_votes.0), HEIGHT, ROUND_0, *VALIDATOR_ID_2),
        )
        .await;
    });

    // Run height - this should trigger storage writes before broadcasts
    let decision = manager
        .run_height(&mut context, HEIGHT, &mut subscriber_channels, &mut proposal_receiver_receiver)
        .await
        .unwrap();

    assert_decision(decision, block_id.0, ROUND_0);
}

#[rstest]
#[tokio::test]
async fn manager_fallback_to_sync_on_height_level_errors(consensus_config: ConsensusConfig) {
    let TestSubscriberChannels { mock_network: _mock_network, subscriber_channels } =
        mock_register_broadcast_topic().unwrap();

    let (mut _proposal_receiver_sender, mut proposal_receiver_receiver) =
        mpsc::channel(CHANNEL_SIZE);

    let mut context = MockTestContext::new();
    context.expect_validators().returning(move |_| Ok(vec![*PROPOSER_ID, *VALIDATOR_ID]));
    context.expect_proposer().returning(move |_, _| Ok(*PROPOSER_ID));
    context.expect_virtual_proposer().returning(move |_, _| Ok(*PROPOSER_ID));

    // Sync should first fail, so consensus will try to run.
    context.expect_try_sync().times(1).returning(|_| false);

    // Consensus should fail when context.set_height_and_round fails.
    context.expect_set_height_and_round().times(1).returning(move |_, _| {
        Err(ConsensusError::BatcherError(BatcherClientError::BatcherError(
            BatcherError::InternalError,
        )))
    });

    // Now sync should be called and succeed.
    context.expect_try_sync().withf(move |height| *height == HEIGHT_1).times(1).returning(|_| true);

    let mut manager = MultiHeightManager::new(
        consensus_config,
        QuorumType::Byzantine,
        Arc::new(Mutex::new(NoOpHeightVotedStorage)),
    )
    .await;
    let res = manager
        .run_height(
            &mut context,
            HEIGHT_1,
            &mut subscriber_channels.into(),
            &mut proposal_receiver_receiver,
        )
        .await;
    assert_eq!(res, Ok(RunHeightRes::Sync));
}

#[rstest]
#[tokio::test]
async fn cache_future_vote_deduplication(consensus_config: ConsensusConfig) {
    let TestSubscriberChannels { mock_network, subscriber_channels } =
        mock_register_broadcast_topic().unwrap();
    let mut sender = mock_network.broadcasted_messages_sender;
    let mut reported_messages_receiver = mock_network.reported_messages_receiver;

    let (mut proposal_receiver_sender, mut proposal_receiver_receiver) =
        mpsc::channel(CHANNEL_SIZE);

    // Send a prevote for HEIGHT_1 (future height - will be cached during HEIGHT_0 processing).
    let original_vote = prevote(Some(Felt::ONE), HEIGHT_1, ROUND_0, *PROPOSER_ID);
    send(&mut sender, original_vote.clone()).await;

    // Send the exact same vote again (duplicate - should be silently ignored, no report).
    send(&mut sender, original_vote).await;

    // Send a conflicting vote: same type/round/voter but different proposal commitment
    // (equivocation - should trigger report_peer).
    let equivocating_vote = prevote(Some(Felt::TWO), HEIGHT_1, ROUND_0, *PROPOSER_ID);
    send(&mut sender, equivocating_vote).await;

    // Send proposal and votes for HEIGHT_0 to reach a decision.
    send_proposal(
        &mut proposal_receiver_sender,
        vec![TestProposalPart::BlockInfo(block_info(HEIGHT_0, ROUND_0, *PROPOSER_ID))],
    )
    .await;
    send(&mut sender, prevote(Some(Felt::ZERO), HEIGHT_0, ROUND_0, *PROPOSER_ID)).await;
    send(&mut sender, precommit(Some(Felt::ZERO), HEIGHT_0, ROUND_0, *PROPOSER_ID)).await;

    let mut context = MockTestContext::new();
    context.expect_try_sync().returning(|_| false);
    expect_validate_proposal(&mut context, Felt::ZERO, 1);
    context.expect_validators().returning(move |_| Ok(vec![*PROPOSER_ID, *VALIDATOR_ID]));
    context.expect_proposer().returning(move |_, _| Ok(*PROPOSER_ID));
    context.expect_virtual_proposer().returning(move |_, _| Ok(*PROPOSER_ID));
    context.expect_set_height_and_round().returning(move |_, _| Ok(()));
    context.expect_broadcast().returning(move |_| Ok(()));
    context
        .expect_decision_reached()
        .withf(move |_, c| *c == ProposalCommitment(Felt::ZERO))
        .return_once(move |_, _| Ok(()));

    let mut manager = MultiHeightManager::new(
        consensus_config,
        QuorumType::Byzantine,
        Arc::new(Mutex::new(NoOpHeightVotedStorage)),
    )
    .await;
    let mut subscriber_channels = subscriber_channels.into();
    let decision = manager
        .run_height(
            &mut context,
            HEIGHT_0,
            &mut subscriber_channels,
            &mut proposal_receiver_receiver,
        )
        .await
        .unwrap();
    assert_decision(decision, Felt::ZERO, ROUND_0);

    // Verify that report_peer was called exactly once (for the equivocation).
    assert!(
        matches!(reported_messages_receiver.try_next(), Ok(Some(_))),
        "Expected report_peer to be called for the equivocation"
    );

    // The duplicate (identical vote) should not have triggered a report.
    assert!(
        reported_messages_receiver.try_next().is_err(),
        "Expected no additional report_peer calls (duplicate should be silently ignored)"
    );
}

#[rstest]
#[tokio::test]
async fn manager_ignores_invalid_network_messages(consensus_config: ConsensusConfig) {
    let TestSubscriberChannels { mock_network, subscriber_channels } =
        mock_register_broadcast_topic().unwrap();
    let mut sender = mock_network.broadcasted_messages_sender;

    let (mut proposal_receiver_sender, mut proposal_receiver_receiver) =
        mpsc::channel(CHANNEL_SIZE);

    let mut context = MockTestContext::new();
    context.expect_validators().returning(move |_| Ok(vec![*PROPOSER_ID, *VALIDATOR_ID]));
    context.expect_proposer().returning(move |_, _| Ok(*PROPOSER_ID));
    context.expect_virtual_proposer().returning(move |_, _| Ok(*PROPOSER_ID));
    context.expect_try_sync().returning(|_| false);

    // Send proposal with no content.
    send_proposal(&mut proposal_receiver_sender, vec![]).await;

    // Send a proposal with invalid Init.
    send_proposal(&mut proposal_receiver_sender, vec![TestProposalPart::Invalid]).await;

    // Send a valid proposal and valid votes.
    send_proposal(
        &mut proposal_receiver_sender,
        vec![TestProposalPart::BlockInfo(block_info(HEIGHT_1, ROUND_0, *PROPOSER_ID))],
    )
    .await;
    send(&mut sender, prevote(Some(Felt::ONE), HEIGHT_1, ROUND_0, *PROPOSER_ID)).await;
    send(&mut sender, precommit(Some(Felt::ONE), HEIGHT_1, ROUND_0, *PROPOSER_ID)).await;

    // TODO(Dafna): Test also invalid votes.

    // Run the manager for height 1.
    expect_validate_proposal(&mut context, Felt::ONE, 1);
    context.expect_set_height_and_round().returning(move |_, _| Ok(()));
    context.expect_broadcast().returning(move |_| Ok(()));
    context
        .expect_decision_reached()
        .withf(move |_, c| *c == ProposalCommitment(Felt::ONE))
        .return_once(move |_, _| Ok(()));
    let mut manager = MultiHeightManager::new(
        consensus_config,
        QuorumType::Byzantine,
        Arc::new(Mutex::new(NoOpHeightVotedStorage)),
    )
    .await;
    let mut subscriber_channels = subscriber_channels.into();
    let decision = manager
        .run_height(
            &mut context,
            HEIGHT_1,
            &mut subscriber_channels,
            &mut proposal_receiver_receiver,
        )
        .await
        .unwrap();
    assert_decision(decision, Felt::ONE, ROUND_0);
}
