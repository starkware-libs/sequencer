use apollo_consensus_config::config::TimeoutsConfig;
use apollo_protobuf::consensus::{ProposalInit, VoteType, DEFAULT_VALIDATOR_ID};
use lazy_static::lazy_static;
use starknet_api::block::BlockNumber;
use starknet_types_core::felt::Felt;
use test_case::test_case;

use super::SingleHeightConsensus;
use crate::single_height_consensus::ShcReturn;
use crate::state_machine::{SMRequest, StateMachineEvent};
use crate::test_utils::{precommit, prevote, TestBlock};
use crate::types::{ProposalCommitment, ValidatorId};
use crate::votes_threshold::QuorumType;

lazy_static! {
    static ref PROPOSER_ID: ValidatorId = DEFAULT_VALIDATOR_ID.into();
    static ref VALIDATOR_ID_1: ValidatorId = (DEFAULT_VALIDATOR_ID + 1).into();
    static ref VALIDATOR_ID_2: ValidatorId = (DEFAULT_VALIDATOR_ID + 2).into();
    static ref VALIDATOR_ID_3: ValidatorId = (DEFAULT_VALIDATOR_ID + 3).into();
    static ref VALIDATORS: Vec<ValidatorId> =
        vec![*PROPOSER_ID, *VALIDATOR_ID_1, *VALIDATOR_ID_2, *VALIDATOR_ID_3];
    static ref BLOCK: TestBlock =
        TestBlock { content: vec![1, 2, 3], id: ProposalCommitment(Felt::ONE) };
    static ref PROPOSAL_INIT: ProposalInit =
        ProposalInit { proposer: *PROPOSER_ID, ..Default::default() };
    static ref TIMEOUTS: TimeoutsConfig = TimeoutsConfig::default();
}
fn get_proposal_init_for_height(height: BlockNumber) -> ProposalInit {
    ProposalInit { height, ..*PROPOSAL_INIT }
}

#[test]
fn proposer() {
    let mut shc = SingleHeightConsensus::new(
        BlockNumber(0),
        false,
        *PROPOSER_ID,
        VALIDATORS.to_vec(),
        QuorumType::Byzantine,
        TIMEOUTS.clone(),
    );
    let leader_fn = |_round| -> ValidatorId { *PROPOSER_ID };
    // Start should request to build proposal.
    let start_ret = shc.start(&leader_fn).unwrap();
    let mut reqs = match start_ret {
        ShcReturn::Requests(r) => r,
        _ => panic!("expected requests"),
    };
    assert!(matches!(reqs.pop_front(), Some(SMRequest::StartBuildProposal(0))));

    // After FinishedBuilding, expect a prevote broadcast request.
    let ret = shc
        .handle_event(&leader_fn, StateMachineEvent::FinishedBuilding(Some(BLOCK.id), 0))
        .unwrap();
    let mut reqs = match ret {
        ShcReturn::Requests(r) => r,
        _ => panic!("expected requests"),
    };
    assert!(matches!(
        reqs.pop_front(),
        Some(SMRequest::BroadcastVote(v)) if v.vote_type == VoteType::Prevote
    ));

    // Receive two prevotes from other validators to reach prevote quorum.
    let _ = shc.handle_vote(&leader_fn, prevote(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_1)).unwrap();
    let ret =
        shc.handle_vote(&leader_fn, prevote(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_2)).unwrap();
    // Expect a precommit broadcast request present.
    let mut reqs = match ret {
        ShcReturn::Requests(r) => r,
        _ => panic!("expected requests"),
    };
    assert!(matches!(reqs.pop_front(), Some(SMRequest::ScheduleTimeoutPrevote { .. })));
    assert!(matches!(
        reqs.pop_front(),
        Some(SMRequest::BroadcastVote(v)) if v.vote_type == VoteType::Precommit
    ));

    // Now provide precommit votes to reach decision.
    let _ =
        shc.handle_vote(&leader_fn, precommit(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_1)).unwrap();
    let decision =
        shc.handle_vote(&leader_fn, precommit(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_2)).unwrap();
    match decision {
        ShcReturn::Decision(d) => assert_eq!(d.block, BLOCK.id),
        _ => panic!("expected decision"),
    }
}

#[test_case(false; "single_proposal")]
#[test_case(true; "repeat_proposal")]
fn validator(repeat_proposal: bool) {
    const HEIGHT: BlockNumber = BlockNumber(0);
    let proposal_init = get_proposal_init_for_height(HEIGHT);
    let mut shc = SingleHeightConsensus::new(
        HEIGHT,
        false,
        *VALIDATOR_ID_1,
        VALIDATORS.to_vec(),
        QuorumType::Byzantine,
        TIMEOUTS.clone(),
    );
    let leader_fn = |_round| -> ValidatorId { *PROPOSER_ID };

    // Accept init -> should request validation.
    let ret = shc.handle_proposal(&leader_fn, proposal_init).unwrap();
    let mut reqs = match ret {
        ShcReturn::Requests(r) => r,
        _ => panic!("expected requests"),
    };
    assert!(matches!(
        reqs.pop_front(),
        Some(SMRequest::StartValidateProposal(init)) if init == proposal_init
    ));

    // After validation finished -> expect prevote broadcast request.
    let ret = shc
        .handle_event(
            &leader_fn,
            StateMachineEvent::FinishedValidation(Some(BLOCK.id), proposal_init.round, None),
        )
        .unwrap();
    let mut reqs = match ret {
        ShcReturn::Requests(r) => r,
        _ => panic!("expected requests"),
    };
    assert!(
        matches!(reqs.pop_front(), Some(SMRequest::BroadcastVote(v)) if v.vote_type == VoteType::Prevote)
    );

    if repeat_proposal {
        // Duplicate proposal init should be ignored.
        let ret = shc.handle_proposal(&leader_fn, proposal_init).unwrap();
        assert!(matches!(ret, ShcReturn::Requests(rs) if rs.is_empty()));
    }

    // Reach prevote quorum with two other validators.
    let _ = shc.handle_vote(&leader_fn, prevote(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_2)).unwrap();
    let ret =
        shc.handle_vote(&leader_fn, prevote(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_3)).unwrap();
    // Expect a precommit broadcast request present.
    let mut reqs = match ret {
        ShcReturn::Requests(r) => r,
        _ => panic!("expected requests"),
    };
    assert!(matches!(reqs.pop_front(), Some(SMRequest::ScheduleTimeoutPrevote { .. })));
    assert!(
        matches!(reqs.pop_front(), Some(SMRequest::BroadcastVote(v)) if v.vote_type == VoteType::Precommit)
    );

    // Now provide precommit votes to reach decision.
    let _ = shc.handle_vote(&leader_fn, precommit(Some(BLOCK.id.0), 0, 0, *PROPOSER_ID)).unwrap();
    let decision =
        shc.handle_vote(&leader_fn, precommit(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_2)).unwrap();
    match decision {
        ShcReturn::Decision(d) => assert_eq!(d.block, BLOCK.id),
        _ => panic!("expected decision"),
    }
}

#[test_case(true; "repeat")]
#[test_case(false; "equivocation")]
fn vote_twice(same_vote: bool) {
    const HEIGHT: BlockNumber = BlockNumber(0);
    let proposal_init = get_proposal_init_for_height(HEIGHT);
    let mut shc = SingleHeightConsensus::new(
        HEIGHT,
        false,
        *VALIDATOR_ID_1,
        VALIDATORS.to_vec(),
        QuorumType::Byzantine,
        TIMEOUTS.clone(),
    );
    let leader_fn = |_round| -> ValidatorId { *PROPOSER_ID };
    // Validate a proposal so the SM is ready to prevote.
    shc.handle_proposal(&leader_fn, proposal_init).unwrap();
    shc.handle_event(
        &leader_fn,
        StateMachineEvent::FinishedValidation(Some(BLOCK.id), proposal_init.round, None),
    )
    .unwrap();

    // Duplicate prevote should be ignored.
    let _ =
        shc.handle_vote(&leader_fn, prevote(Some(BLOCK.id.0), HEIGHT.0, 0, *PROPOSER_ID)).unwrap();
    let res = shc
        .handle_vote(&leader_fn, prevote(Some(BLOCK.id.0), HEIGHT.0, 0, *VALIDATOR_ID_2))
        .unwrap();
    // On quorum of prevotes, expect a precommit broadcast request.
    let mut reqs = match res {
        ShcReturn::Requests(r) => r,
        _ => panic!("expected requests"),
    };
    assert!(matches!(reqs.pop_front(), Some(SMRequest::ScheduleTimeoutPrevote { .. })));
    assert!(matches!(
        reqs.pop_front(),
        Some(SMRequest::BroadcastVote(v)) if v.vote_type == VoteType::Precommit
    ));

    // Precommit handling towards decision.
    let first_vote = precommit(Some(BLOCK.id.0), HEIGHT.0, 0, *PROPOSER_ID);
    let _ = shc.handle_vote(&leader_fn, first_vote.clone()).unwrap();
    let second_vote = if same_vote {
        first_vote.clone()
    } else {
        precommit(Some(Felt::TWO), HEIGHT.0, 0, *PROPOSER_ID)
    };
    let _ = shc.handle_vote(&leader_fn, second_vote.clone()).unwrap();
    let decision =
        shc.handle_vote(&leader_fn, precommit(Some(BLOCK.id.0), HEIGHT.0, 0, *VALIDATOR_ID_3));
    match decision {
        Ok(ShcReturn::Decision(d)) => assert_eq!(d.block, BLOCK.id),
        _ => panic!("Expected decision"),
    }
}

#[test]
fn rebroadcast_votes() {
    let mut shc = SingleHeightConsensus::new(
        BlockNumber(0),
        false,
        *PROPOSER_ID,
        VALIDATORS.to_vec(),
        QuorumType::Byzantine,
        TIMEOUTS.clone(),
    );
    let leader_fn = |_round| -> ValidatorId { *PROPOSER_ID };
    // Start and build.
    let _ = shc.start(&leader_fn).unwrap();

    let ret = shc
        .handle_event(&leader_fn, StateMachineEvent::FinishedBuilding(Some(BLOCK.id), 0))
        .unwrap();
    let mut reqs = match ret {
        ShcReturn::Requests(r) => r,
        _ => panic!("expected requests"),
    };
    assert!(matches!(
        reqs.pop_front(),
        Some(SMRequest::BroadcastVote(v)) if v.vote_type == VoteType::Prevote
    ));

    // Receive two prevotes from other validators to reach prevote quorum at round 0.
    let _ = shc.handle_vote(&leader_fn, prevote(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_1)).unwrap();
    let ret =
        shc.handle_vote(&leader_fn, prevote(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_2)).unwrap();
    // Expect a precommit broadcast at round 0.
    let reqs = match ret {
        ShcReturn::Requests(r) => r,
        _ => panic!("expected requests"),
    };
    assert!(reqs.iter().any(|req| matches!(
        req,
        SMRequest::BroadcastVote(v) if v.vote_type == VoteType::Precommit && v.round == 0
    )));

    // Advance with NIL precommits from peers (no decision) -> expect scheduling of precommit
    // timeout.
    let _ = shc.handle_vote(&leader_fn, precommit(None, 0, 0, *VALIDATOR_ID_1)).unwrap();
    let _ = shc.handle_vote(&leader_fn, precommit(None, 0, 0, *VALIDATOR_ID_2)).unwrap();

    // Timeout at precommit(0) -> expect a prevote broadcast for round 1.
    let ret = shc.handle_event(&leader_fn, StateMachineEvent::TimeoutPrecommit(0)).unwrap();
    let reqs = match ret {
        ShcReturn::Requests(r) => r,
        _ => panic!("expected requests"),
    };
    assert!(reqs.iter().any(|req| matches!(
        req,
        SMRequest::BroadcastVote(v) if v.vote_type == VoteType::Prevote && v.round == 1
    )));

    // Reach prevote quorum at round 1 with two other validators.
    let _ = shc.handle_vote(&leader_fn, prevote(Some(BLOCK.id.0), 0, 1, *VALIDATOR_ID_2)).unwrap();
    let ret =
        shc.handle_vote(&leader_fn, prevote(Some(BLOCK.id.0), 0, 1, *VALIDATOR_ID_3)).unwrap();
    let reqs = match ret {
        ShcReturn::Requests(r) => r,
        _ => panic!("expected requests"),
    };
    assert!(reqs.iter().any(|req| matches!(
        req,
        SMRequest::BroadcastVote(v) if v.vote_type == VoteType::Precommit && v.round == 1
    )));

    // Rebroadcast with older vote (round 0) - should be ignored (no broadcast, no task).
    let ret = shc.handle_event(
        &leader_fn,
        StateMachineEvent::VoteBroadcasted(precommit(Some(BLOCK.id.0), 0, 0, *PROPOSER_ID)),
    );
    match ret.unwrap() {
        ShcReturn::Requests(r) => assert!(r.is_empty()),
        _ => panic!("expected requests"),
    }

    // Rebroadcast with current round (round 1) - should broadcast.
    let rebroadcast_vote = precommit(Some(BLOCK.id.0), 0, 1, *PROPOSER_ID);
    let ret = shc
        .handle_event(&leader_fn, StateMachineEvent::VoteBroadcasted(rebroadcast_vote.clone()))
        .unwrap();
    let mut reqs = match ret {
        ShcReturn::Requests(r) => r,
        _ => panic!("expected requests"),
    };
    // Expect BroadcastVote for the latest vote (round 1).
    assert!(matches!(
        reqs.pop_front(),
        Some(SMRequest::BroadcastVote(v)) if v == rebroadcast_vote
    ));
}

#[test]
fn repropose() {
    let mut shc = SingleHeightConsensus::new(
        BlockNumber(0),
        false,
        *PROPOSER_ID,
        VALIDATORS.to_vec(),
        QuorumType::Byzantine,
        TIMEOUTS.clone(),
    );
    let leader_fn = |_round| -> ValidatorId { *PROPOSER_ID };
    let _ = shc.start(&leader_fn).unwrap();
    // After building the proposal, the proposer broadcasts a prevote for round 0.
    let ret = shc
        .handle_event(&leader_fn, StateMachineEvent::FinishedBuilding(Some(BLOCK.id), 0))
        .unwrap();
    // Expect a BroadcastVote(Prevote) request for round 0.
    match ret {
        ShcReturn::Requests(reqs) => {
            assert!(reqs.iter().any(|req| matches!(
                req,
                SMRequest::BroadcastVote(v) if v.vote_type == VoteType::Prevote && v.round == 0
            )));
        }
        _ => panic!("expected requests"),
    }
    // A single prevote from another validator does not yet cause quorum.
    let ret =
        shc.handle_vote(&leader_fn, prevote(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_1)).unwrap();
    // No new requests are expected at this point.
    match ret {
        ShcReturn::Requests(reqs) => assert!(reqs.is_empty()),
        _ => panic!("expected requests"),
    }
    // Reaching prevote quorum with a second external prevote; proposer will broadcast a precommit
    // and schedule a prevote timeout for round 0.
    let ret =
        shc.handle_vote(&leader_fn, prevote(Some(BLOCK.id.0), 0, 0, *VALIDATOR_ID_2)).unwrap();
    // Expect ScheduleTimeoutPrevote{round:0} and BroadcastVote(Precommit).
    match ret {
        ShcReturn::Requests(reqs) => {
            assert!(reqs.iter().any(|req| matches!(req, SMRequest::ScheduleTimeoutPrevote(0))));
            assert!(reqs.iter().any(|req| matches!(
                req,
                SMRequest::BroadcastVote(v) if v.vote_type == VoteType::Precommit && v.round == 0
            )));
        }
        _ => panic!("expected requests"),
    }
    // receiving Nil precommit requests and then decision on new round; just assert no panic and
    // decisions arrive after quorum.
    let _ = shc.handle_vote(&leader_fn, precommit(None, 0, 0, *VALIDATOR_ID_1)).unwrap();
    let ret = shc.handle_vote(&leader_fn, precommit(None, 0, 0, *VALIDATOR_ID_2)).unwrap();
    // assert that ret is ScheduleTimeoutPrecommit
    match ret {
        ShcReturn::Requests(reqs) => {
            assert!(reqs.iter().any(|req| matches!(req, SMRequest::ScheduleTimeoutPrecommit(0))));
        }
        _ => panic!("expected requests"),
    }
    // No precommit quorum is reached. On TimeoutPrecommit(0) the proposer advances to round 1 with
    // a valid value (valid_round = Some(0)) and reproposes the same block, then broadcasts a
    // new prevote for round 1.
    let ret = shc.handle_event(&leader_fn, StateMachineEvent::TimeoutPrecommit(0)).unwrap();
    // Expect Repropose with init.round == 1, init.valid_round == Some(0), and a
    // BroadcastVote(Prevote) for round 1.
    match ret {
        ShcReturn::Requests(reqs) => {
            assert!(reqs.iter().any(|req| matches!(
                req,
                SMRequest::Repropose(proposal_id, init) if *proposal_id == BLOCK.id && init.round == 1 && init.valid_round == Some(0)
            )));
            assert!(reqs.iter().any(|req| matches!(
                req,
                SMRequest::BroadcastVote(v) if v.vote_type == VoteType::Prevote && v.round == 1
            )));
        }
        _ => panic!("expected requests"),
    }
}
