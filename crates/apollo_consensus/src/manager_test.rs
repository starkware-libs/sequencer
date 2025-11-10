use std::sync::Arc;
use std::time::Duration;
use std::vec;

use apollo_config_manager_types::communication::MockConfigManagerClient;
use apollo_consensus_config::config::{
    ConsensusConfig,
    ConsensusDynamicConfig,
    ConsensusStaticConfig,
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
use apollo_test_utils::{get_rng, GetTestInstance};
use futures::channel::{mpsc, oneshot};
use futures::{FutureExt, SinkExt};
use lazy_static::lazy_static;
use rstest::{fixture, rstest};
use starknet_api::block::BlockNumber;
use starknet_types_core::felt::Felt;

use super::{run_consensus, MultiHeightManager, RunHeightRes};
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
        prevote: Timeout {
            base: Duration::from_millis(100),
            delta: Duration::from_millis(10),
            max: Duration::from_millis(500),
        },
        precommit: Timeout {
            base: Duration::from_millis(100),
            delta: Duration::from_millis(10),
            max: Duration::from_millis(500),
        },
        proposal: Timeout {
            base: Duration::from_millis(100),
            delta: Duration::from_millis(10),
            max: Duration::from_millis(1000),
        },
    };
}

const CHANNEL_SIZE: usize = 10;
const SYNC_RETRY_INTERVAL: Duration = Duration::from_millis(100);

#[fixture]
fn consensus_config() -> ConsensusConfig {
    ConsensusConfig::from_parts(
        ConsensusDynamicConfig {
            validator_id: *VALIDATOR_ID,
            timeouts: TIMEOUTS.clone(),
            sync_retry_interval: SYNC_RETRY_INTERVAL,
        },
        ConsensusStaticConfig { startup_delay: Duration::ZERO, ..Default::default() },
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

fn assert_decision(res: RunHeightRes, id: Felt) {
    match res {
        RunHeightRes::Decision(decision) => assert_eq!(decision.block, ProposalCommitment(id)),
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

    let mut manager = MultiHeightManager::new(consensus_config, QuorumType::Byzantine);
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
    assert_decision(decision, Felt::ONE);

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
    assert_decision(decision, Felt::TWO);
}

#[rstest]
#[tokio::test]
async fn run_consensus_sync(consensus_config: ConsensusConfig) {
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
        .withf(move |block, votes| *block == ProposalCommitment(Felt::TWO) && votes[0].height == 2)
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
        consensus_config,
        start_active_height: BlockNumber(1),
        start_observe_height: BlockNumber(1),
        quorum_type: QuorumType::Byzantine,
        config_manager_client: None,
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

    // Ensure our validator id matches the expectation in the broadcast assertion.
    let mut manager = MultiHeightManager::new(consensus_config, QuorumType::Byzantine);
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

#[rstest]
#[tokio::test]
async fn timely_message_handling(consensus_config: ConsensusConfig) {
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

    let mut manager = MultiHeightManager::new(consensus_config, QuorumType::Byzantine);
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
    context.expect_set_height_and_round().returning(move |_, _| ());
    context.expect_validators().returning(move |h: BlockNumber| {
        if h == BlockNumber(1) { vec![*VALIDATOR_ID] } else { vec![*PROPOSER_ID] }
    });
    context.expect_proposer().returning(move |h: BlockNumber, _| {
        if h == BlockNumber(1) { *VALIDATOR_ID } else { *PROPOSER_ID }
    });
    context.expect_try_sync().withf(move |h| *h == BlockNumber(1)).times(1).returning(|_| true);
    context.expect_try_sync().returning(|_| false);
    context.expect_broadcast().returning(move |_| Ok(()));

    // In this test, build_proposal should be called only when the dynamic config returns that we
    // are the proposer, which happens at H2.
    context
        .expect_build_proposal()
        .withf(move |init, _| init.height == BlockNumber(2) && init.proposer == *PROPOSER_ID)
        .returning(move |_, _| {
            let (sender, receiver) = oneshot::channel();
            sender.send(ProposalCommitment(Felt::TWO)).unwrap();
            receiver
        })
        .times(1);
    // Expect a decision at height 2.
    let (decision_tx, decision_rx) = oneshot::channel();
    context
        .expect_decision_reached()
        .withf(move |_, votes| votes.first().map(|v| v.height) == Some(2))
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
        start_active_height: BlockNumber(1),
        start_observe_height: BlockNumber(1),
        consensus_config,
        quorum_type: QuorumType::Byzantine,
        config_manager_client: Some(Arc::new(mock_client)),
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
