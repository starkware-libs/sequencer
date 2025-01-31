//! Run a single height of consensus.
//!
//! [`SingleHeightConsensus`] (SHC) - run consensus for a single height.
//!
//! [`ShcTask`] - a task which should be run without blocking consensus.
//!
//! [`ShcEvent`] - an event, generated from an `ShcTask` which should be handled by the SHC.

#[cfg(test)]
#[path = "single_height_consensus_test.rs"]
mod single_height_consensus_test;

use std::collections::hash_map::Entry;
use std::collections::{HashMap, VecDeque};
use std::time::Duration;

#[cfg(test)]
use enum_as_inner::EnumAsInner;
use futures::channel::{mpsc, oneshot};
use papyrus_protobuf::consensus::{ProposalFin, ProposalInit, Vote, VoteType};
use starknet_api::block::BlockNumber;
use tracing::{debug, info, instrument, trace, warn};

use crate::config::TimeoutsConfig;
use crate::state_machine::{StateMachine, StateMachineEvent};
use crate::types::{
    ConsensusContext,
    ConsensusError,
    Decision,
    ProposalCommitment,
    Round,
    ValidatorId,
};

/// The SHC can either update the manager of a decision or return tasks that should be run without
/// blocking further calls to itself.
#[derive(Debug, PartialEq)]
#[cfg_attr(test, derive(EnumAsInner))]
pub enum ShcReturn {
    Tasks(Vec<ShcTask>),
    Decision(Decision),
}

/// Events produced from tasks for the SHC to handle.
#[derive(Debug, Clone)]
pub enum ShcEvent {
    TimeoutPropose(StateMachineEvent),
    TimeoutPrevote(StateMachineEvent),
    TimeoutPrecommit(StateMachineEvent),
    Prevote(StateMachineEvent),
    Precommit(StateMachineEvent),
    BuildProposal(StateMachineEvent),
    // TODO(Matan): Replace ProposalCommitment with the unvalidated signature from the proposer.
    ValidateProposal(StateMachineEvent, Option<ProposalFin>),
}

/// A task which should be run without blocking calls to SHC.
#[derive(Debug)]
#[cfg_attr(test, derive(EnumAsInner))]
pub enum ShcTask {
    TimeoutPropose(Duration, StateMachineEvent),
    TimeoutPrevote(Duration, StateMachineEvent),
    TimeoutPrecommit(Duration, StateMachineEvent),
    Prevote(Duration, StateMachineEvent),
    Precommit(Duration, StateMachineEvent),
    /// Building a proposal is handled in 3 stages:
    /// 1. The SHC requests a block to be built from the context.
    /// 2. SHC returns, allowing the context to build the block while the Manager awaits the result
    ///    without blocking consensus.
    /// 3. Once building is complete, the manager returns the built block to the SHC as an event,
    ///    which can be sent to the SM.
    /// * During this process, the SM is frozen; it will accept and buffer other events, only
    ///   processing them once it receives the built proposal.
    BuildProposal(Round, oneshot::Receiver<ProposalCommitment>),
    /// Validating a proposal is handled in 3 stages:
    /// 1. The SHC validates `ProposalInit`, then starts block validation within the context.
    /// 2. SHC returns, allowing the context to validate the content while the Manager await the
    ///    result without blocking consensus.
    /// 3. Once validation is complete, the manager returns the built proposal to the SHC as an
    ///    event, which can be sent to the SM.
    ValidateProposal(ProposalInit, oneshot::Receiver<(ProposalCommitment, ProposalFin)>),
}

impl PartialEq for ShcTask {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ShcTask::TimeoutPropose(d1, e1), ShcTask::TimeoutPropose(d2, e2))
            | (ShcTask::TimeoutPrevote(d1, e1), ShcTask::TimeoutPrevote(d2, e2))
            | (ShcTask::TimeoutPrecommit(d1, e1), ShcTask::TimeoutPrecommit(d2, e2))
            | (ShcTask::Prevote(d1, e1), ShcTask::Prevote(d2, e2))
            | (ShcTask::Precommit(d1, e1), ShcTask::Precommit(d2, e2)) => d1 == d2 && e1 == e2,
            (ShcTask::BuildProposal(r1, _), ShcTask::BuildProposal(r2, _)) => r1 == r2,
            (ShcTask::ValidateProposal(pi1, _), ShcTask::ValidateProposal(pi2, _)) => pi1 == pi2,
            _ => false,
        }
    }
}

impl ShcTask {
    pub async fn run(self) -> ShcEvent {
        trace!("Running task: {:?}", self);
        match self {
            ShcTask::TimeoutPropose(duration, event) => {
                tokio::time::sleep(duration).await;
                ShcEvent::TimeoutPropose(event)
            }
            ShcTask::TimeoutPrevote(duration, event) => {
                tokio::time::sleep(duration).await;
                ShcEvent::TimeoutPrevote(event)
            }
            ShcTask::TimeoutPrecommit(duration, event) => {
                tokio::time::sleep(duration).await;
                ShcEvent::TimeoutPrecommit(event)
            }
            ShcTask::Prevote(duration, event) => {
                tokio::time::sleep(duration).await;
                ShcEvent::Prevote(event)
            }
            ShcTask::Precommit(duration, event) => {
                tokio::time::sleep(duration).await;
                ShcEvent::Precommit(event)
            }
            ShcTask::BuildProposal(round, receiver) => {
                let proposal_id = receiver.await.ok();
                ShcEvent::BuildProposal(StateMachineEvent::GetProposal(proposal_id, round))
            }
            ShcTask::ValidateProposal(init, block_receiver) => {
                // Handle the result of the block validation:
                // The output is a tuple with the proposal id, calculated and from network.
                // - If successful, set it as (Some, Some).
                // - If there was an error (e.g., invalid proposal, no proposal received from the
                //   peer, or the process was interrupted), set it to (None, None).
                // TODO(Asmaa): Consider if we want to differentiate between an interrupt and other
                // failures.
                let (built_content_id, received_proposal_id) = match block_receiver.await {
                    Ok((built_content_id, received_proposal_id)) => {
                        (Some(built_content_id), Some(received_proposal_id))
                    }
                    // Proposal never received from peer.
                    Err(_) => (None, None),
                };
                ShcEvent::ValidateProposal(
                    StateMachineEvent::Proposal(built_content_id, init.round, init.valid_round),
                    received_proposal_id,
                )
            }
        }
    }
}

/// Represents a single height of consensus. It is responsible for mapping between the idealized
/// view of consensus represented in the StateMachine and the real world implementation.
///
/// Example:
/// - Timeouts: the SM returns an event timeout, but SHC then maps that to a task which can be run
///   by the Manager. The manager though unaware of the specific task as it has minimal consensus
///   logic.
///
/// Each height is begun with a call to `start`, with no further calls to it.
///
/// SHC is not a top level task, it is called directly and returns values (doesn't directly run sub
/// tasks). SHC does have side effects, such as sending messages to the network via the context.
pub(crate) struct SingleHeightConsensus {
    height: BlockNumber,
    validators: Vec<ValidatorId>,
    id: ValidatorId,
    timeouts: TimeoutsConfig,
    state_machine: StateMachine,
    proposals: HashMap<Round, Option<ProposalCommitment>>,
    prevotes: HashMap<(Round, ValidatorId), Vote>,
    precommits: HashMap<(Round, ValidatorId), Vote>,
    last_prevote: Option<Vote>,
    last_precommit: Option<Vote>,
}

impl SingleHeightConsensus {
    pub(crate) fn new(
        height: BlockNumber,
        is_observer: bool,
        id: ValidatorId,
        validators: Vec<ValidatorId>,
        timeouts: TimeoutsConfig,
    ) -> Self {
        // TODO(matan): Use actual weights, not just `len`.
        let n_validators =
            u32::try_from(validators.len()).expect("Should have way less than u32::MAX validators");
        let state_machine = StateMachine::new(id, n_validators, is_observer);
        Self {
            height,
            validators,
            id,
            timeouts,
            state_machine,
            proposals: HashMap::new(),
            prevotes: HashMap::new(),
            precommits: HashMap::new(),
            last_prevote: None,
            last_precommit: None,
        }
    }

    #[instrument(skip_all)]
    pub(crate) async fn start<ContextT: ConsensusContext>(
        &mut self,
        context: &mut ContextT,
    ) -> Result<ShcReturn, ConsensusError> {
        context.set_height_and_round(self.height, self.state_machine.round()).await;
        let leader_fn = |round: Round| -> ValidatorId { context.proposer(self.height, round) };
        let events = self.state_machine.start(&leader_fn);
        let ret = self.handle_state_machine_events(context, events).await;
        context.set_height_and_round(self.height, self.state_machine.round()).await;
        ret
    }

    /// Process the proposal init and initiate block validation. See [`ShcTask::ValidateProposal`]
    /// for more details on the full proposal flow.
    #[instrument(skip_all)]
    pub(crate) async fn handle_proposal<ContextT: ConsensusContext>(
        &mut self,
        context: &mut ContextT,
        init: ProposalInit,
        p2p_messages_receiver: mpsc::Receiver<ContextT::ProposalPart>,
    ) -> Result<ShcReturn, ConsensusError> {
        debug!("Received {init:?}");
        let proposer_id = context.proposer(self.height, init.round);
        if init.height != self.height {
            warn!("Invalid proposal height: expected {:?}, got {:?}", self.height, init.height);
            return Ok(ShcReturn::Tasks(Vec::new()));
        }
        if init.proposer != proposer_id {
            warn!("Invalid proposer: expected {:?}, got {:?}", proposer_id, init.proposer);
            return Ok(ShcReturn::Tasks(Vec::new()));
        }
        let Entry::Vacant(proposal_entry) = self.proposals.entry(init.round) else {
            warn!("Round {} already has a proposal, ignoring", init.round);
            return Ok(ShcReturn::Tasks(Vec::new()));
        };
        let timeout = self.timeouts.proposal_timeout;
        info!(
            "Accepting {init:?}. node_round: {}, timeout: {timeout:?}",
            self.state_machine.round()
        );
        // Since validating the proposal is non-blocking, we want to avoid validating the same round
        // twice in parallel. This could be caused by a network repeat or a malicious spam attack.
        proposal_entry.insert(None);
        let block_receiver = context.validate_proposal(init, timeout, p2p_messages_receiver).await;
        context.set_height_and_round(self.height, self.state_machine.round()).await;
        Ok(ShcReturn::Tasks(vec![ShcTask::ValidateProposal(init, block_receiver)]))
    }

    #[instrument(skip_all)]
    pub async fn handle_event<ContextT: ConsensusContext>(
        &mut self,
        context: &mut ContextT,
        event: ShcEvent,
    ) -> Result<ShcReturn, ConsensusError> {
        debug!("Received ShcEvent: {:?}", event);
        let ret = match event {
            ShcEvent::TimeoutPropose(event)
            | ShcEvent::TimeoutPrevote(event)
            | ShcEvent::TimeoutPrecommit(event) => {
                let leader_fn =
                    |round: Round| -> ValidatorId { context.proposer(self.height, round) };
                let sm_events = self.state_machine.handle_event(event, &leader_fn);
                self.handle_state_machine_events(context, sm_events).await
            }
            ShcEvent::Prevote(StateMachineEvent::Prevote(proposal_id, round)) => {
                let Some(last_vote) = &self.last_prevote else {
                    return Err(ConsensusError::InternalInconsistency(
                        "No prevote to send".to_string(),
                    ));
                };
                if last_vote.round > round {
                    // Only replay the newest prevote.
                    return Ok(ShcReturn::Tasks(Vec::new()));
                }
                debug!("Rebroadcasting {last_vote:?}");
                context.broadcast(last_vote.clone()).await?;
                Ok(ShcReturn::Tasks(vec![ShcTask::Prevote(
                    self.timeouts.prevote_timeout,
                    StateMachineEvent::Prevote(proposal_id, round),
                )]))
            }
            ShcEvent::Precommit(StateMachineEvent::Precommit(proposal_id, round)) => {
                let Some(last_vote) = &self.last_precommit else {
                    return Err(ConsensusError::InternalInconsistency(
                        "No precommit to send".to_string(),
                    ));
                };
                if last_vote.round > round {
                    // Only replay the newest precommit.
                    return Ok(ShcReturn::Tasks(Vec::new()));
                }
                debug!("Rebroadcasting {last_vote:?}");
                context.broadcast(last_vote.clone()).await?;
                Ok(ShcReturn::Tasks(vec![ShcTask::Precommit(
                    self.timeouts.precommit_timeout,
                    StateMachineEvent::Precommit(proposal_id, round),
                )]))
            }
            ShcEvent::ValidateProposal(
                StateMachineEvent::Proposal(built_id, round, valid_round),
                received_fin,
            ) => {
                let leader_fn =
                    |round: Round| -> ValidatorId { context.proposer(self.height, round) };
                debug!(
                    proposer = %leader_fn(round),
                    %round,
                    ?valid_round,
                    ?built_id,
                    ?received_fin,
                    node_round = self.state_machine.round(),
                    "Validated proposal.",
                );
                // TODO(matan): Switch to signature validation.
                if built_id != received_fin.as_ref().map(|fin| fin.proposal_commitment) {
                    warn!("proposal_id built from content received does not match fin.");
                    return Ok(ShcReturn::Tasks(Vec::new()));
                }
                // Retaining the entry for this round prevents us from receiving another proposal on
                // this round. While this prevents spam attacks it also prevents re-receiving after
                // a network issue.
                let old = self.proposals.insert(round, built_id);
                let old = old.unwrap_or_else(|| {
                    panic!("Proposal entry should exist from init. round: {round}")
                });
                assert!(old.is_none(), "Proposal already exists for this round: {round}. {old:?}");
                let sm_events = self.state_machine.handle_event(
                    StateMachineEvent::Proposal(built_id, round, valid_round),
                    &leader_fn,
                );
                self.handle_state_machine_events(context, sm_events).await
            }
            ShcEvent::BuildProposal(StateMachineEvent::GetProposal(proposal_id, round)) => {
                let old = self.proposals.insert(round, proposal_id);
                assert!(old.is_none(), "There should be no entry for round {round} when proposing");
                assert_eq!(
                    round,
                    self.state_machine.round(),
                    "State machine should not progress while awaiting proposal"
                );
                debug!(%round, proposal_commitment = ?proposal_id, "Built proposal.");
                let leader_fn =
                    |round: Round| -> ValidatorId { context.proposer(self.height, round) };
                let sm_events = self
                    .state_machine
                    .handle_event(StateMachineEvent::GetProposal(proposal_id, round), &leader_fn);
                self.handle_state_machine_events(context, sm_events).await
            }
            _ => unimplemented!("Unexpected event: {:?}", event),
        };
        context.set_height_and_round(self.height, self.state_machine.round()).await;
        ret
    }

    /// Handle vote messages from peer nodes.
    #[instrument(skip_all)]
    pub(crate) async fn handle_vote<ContextT: ConsensusContext>(
        &mut self,
        context: &mut ContextT,
        vote: Vote,
    ) -> Result<ShcReturn, ConsensusError> {
        debug!("Received {:?}", vote);
        if !self.validators.contains(&vote.voter) {
            debug!("Ignoring vote from non validator: vote={:?}", vote);
            return Ok(ShcReturn::Tasks(Vec::new()));
        }

        let (votes, sm_vote) = match vote.vote_type {
            VoteType::Prevote => {
                (&mut self.prevotes, StateMachineEvent::Prevote(vote.block_hash, vote.round))
            }
            VoteType::Precommit => {
                (&mut self.precommits, StateMachineEvent::Precommit(vote.block_hash, vote.round))
            }
        };

        match votes.entry((vote.round, vote.voter)) {
            Entry::Vacant(entry) => {
                entry.insert(vote.clone());
            }
            Entry::Occupied(entry) => {
                let old = entry.get();
                if old.block_hash != vote.block_hash {
                    warn!("Conflicting votes: old={:?}, new={:?}", old, vote);
                    return Ok(ShcReturn::Tasks(Vec::new()));
                } else {
                    // Replay, ignore.
                    return Ok(ShcReturn::Tasks(Vec::new()));
                }
            }
        }
        info!("Accepting {:?}", vote);
        let leader_fn = |round: Round| -> ValidatorId { context.proposer(self.height, round) };
        let sm_events = self.state_machine.handle_event(sm_vote, &leader_fn);
        let ret = self.handle_state_machine_events(context, sm_events).await;
        context.set_height_and_round(self.height, self.state_machine.round()).await;
        ret
    }

    // Handle events output by the state machine.
    async fn handle_state_machine_events<ContextT: ConsensusContext>(
        &mut self,
        context: &mut ContextT,
        mut events: VecDeque<StateMachineEvent>,
    ) -> Result<ShcReturn, ConsensusError> {
        let mut ret_val = Vec::new();
        while let Some(event) = events.pop_front() {
            trace!("Handling sm event: {:?}", event);
            match event {
                StateMachineEvent::GetProposal(proposal_id, round) => {
                    ret_val.extend(
                        self.handle_state_machine_get_proposal(context, proposal_id, round).await,
                    );
                }
                StateMachineEvent::Proposal(proposal_id, round, valid_round) => {
                    self.handle_state_machine_proposal(context, proposal_id, round, valid_round)
                        .await;
                }
                StateMachineEvent::Decision(proposal_id, round) => {
                    return self.handle_state_machine_decision(proposal_id, round).await;
                }
                StateMachineEvent::Prevote(proposal_id, round) => {
                    ret_val.extend(
                        self.handle_state_machine_vote(
                            context,
                            proposal_id,
                            round,
                            VoteType::Prevote,
                        )
                        .await?,
                    );
                }
                StateMachineEvent::Precommit(proposal_id, round) => {
                    ret_val.extend(
                        self.handle_state_machine_vote(
                            context,
                            proposal_id,
                            round,
                            VoteType::Precommit,
                        )
                        .await?,
                    );
                }
                StateMachineEvent::TimeoutPropose(_) => {
                    ret_val.push(ShcTask::TimeoutPropose(self.timeouts.proposal_timeout, event));
                }
                StateMachineEvent::TimeoutPrevote(_) => {
                    ret_val.push(ShcTask::TimeoutPrevote(self.timeouts.prevote_timeout, event));
                }
                StateMachineEvent::TimeoutPrecommit(_) => {
                    ret_val.push(ShcTask::TimeoutPrecommit(self.timeouts.precommit_timeout, event));
                }
            }
        }
        Ok(ShcReturn::Tasks(ret_val))
    }

    /// Initiate block building. See [`ShcTask::BuildProposal`] for more details on the full
    /// proposal flow.
    async fn handle_state_machine_get_proposal<ContextT: ConsensusContext>(
        &mut self,
        context: &mut ContextT,
        proposal_id: Option<ProposalCommitment>,
        round: Round,
    ) -> Vec<ShcTask> {
        assert!(
            proposal_id.is_none(),
            "StateMachine is requesting a new proposal, but provided a content id."
        );

        // TODO(Matan): Figure out how to handle failed proposal building. I believe this should be
        // handled by applying timeoutPropose when we are the leader.
        let init =
            ProposalInit { height: self.height, round, proposer: self.id, valid_round: None };
        let fin_receiver = context.build_proposal(init, self.timeouts.proposal_timeout).await;
        vec![ShcTask::BuildProposal(round, fin_receiver)]
    }

    async fn handle_state_machine_proposal<ContextT: ConsensusContext>(
        &mut self,
        context: &mut ContextT,
        proposal_id: Option<ProposalCommitment>,
        round: Round,
        valid_round: Option<Round>,
    ) {
        let Some(valid_round) = valid_round else {
            // Newly built proposals are handled by the BuildProposal flow.
            return;
        };
        let proposal_id = proposal_id.expect("Reproposal must have a valid ID");

        let id = self
            .proposals
            .get(&valid_round)
            .unwrap_or_else(|| panic!("A proposal should exist for valid_round: {valid_round}"))
            .unwrap_or_else(|| {
                panic!("A valid proposal should exist for valid_round: {valid_round}")
            });
        assert_eq!(id, proposal_id, "reproposal should match the stored proposal");
        let old = self.proposals.insert(round, Some(proposal_id));
        assert!(old.is_none(), "There should be no proposal for round {round}.");
        let init = ProposalInit {
            height: self.height,
            round,
            proposer: self.id,
            valid_round: Some(valid_round),
        };
        context.repropose(id, init).await;
    }

    async fn handle_state_machine_vote<ContextT: ConsensusContext>(
        &mut self,
        context: &mut ContextT,
        proposal_id: Option<ProposalCommitment>,
        round: Round,
        vote_type: VoteType,
    ) -> Result<Vec<ShcTask>, ConsensusError> {
        let (votes, last_vote, task) = match vote_type {
            VoteType::Prevote => (
                &mut self.prevotes,
                &mut self.last_prevote,
                ShcTask::Prevote(
                    self.timeouts.prevote_timeout,
                    StateMachineEvent::Prevote(proposal_id, round),
                ),
            ),
            VoteType::Precommit => (
                &mut self.precommits,
                &mut self.last_precommit,
                ShcTask::Precommit(
                    self.timeouts.precommit_timeout,
                    StateMachineEvent::Precommit(proposal_id, round),
                ),
            ),
        };
        let vote = Vote {
            vote_type,
            height: self.height.0,
            round,
            block_hash: proposal_id,
            voter: self.id,
        };
        if let Some(old) = votes.insert((round, self.id), vote.clone()) {
            return Err(ConsensusError::InternalInconsistency(format!(
                "State machine should not send repeat votes: old={:?}, new={:?}",
                old, vote
            )));
        }
        *last_vote = match last_vote {
            None => Some(vote.clone()),
            Some(last_vote) if round > last_vote.round => Some(vote.clone()),
            Some(_) => {
                // According to the Tendermint paper, the state machine should only vote for its
                // current round. It should monotonicly increase its round. It should only vote once
                // per step.
                return Err(ConsensusError::InternalInconsistency(format!(
                    "State machine must progress in time: last_vote: {:?} new_vote: {:?}",
                    last_vote, vote,
                )));
            }
        };

        info!("Broadcasting {vote:?}");
        context.broadcast(vote).await?;
        Ok(vec![task])
    }

    async fn handle_state_machine_decision(
        &mut self,
        proposal_id: ProposalCommitment,
        round: Round,
    ) -> Result<ShcReturn, ConsensusError> {
        let invalid_decision = |msg: String| {
            ConsensusError::InternalInconsistency(format!(
                "Invalid decision: sm_proposal_id: {proposal_id}, round: {round}. {msg}",
            ))
        };
        let block = self
            .proposals
            .remove(&round)
            .ok_or_else(|| invalid_decision("No proposal entry for this round".to_string()))?
            .ok_or_else(|| {
                invalid_decision(
                    "Proposal is invalid or validations haven't yet completed".to_string(),
                )
            })?;
        if block != proposal_id {
            return Err(invalid_decision(format!(
                "StateMachine block hash should match the stored block. Shc.block_id: {block}"
            )));
        }
        let supporting_precommits: Vec<Vote> = self
            .validators
            .iter()
            .filter_map(|v| {
                let vote = self.precommits.get(&(round, *v))?;
                if vote.block_hash == Some(proposal_id) { Some(vote.clone()) } else { None }
            })
            .collect();
        let quorum_size =
            usize::try_from(self.state_machine.quorum_size()).expect("u32 should fit in usize");
        // TODO(matan): Check actual weights.
        if quorum_size > supporting_precommits.len() {
            let msg = format!(
                "Not enough supporting votes. quorum_size: {quorum_size}, num_supporting_votes: \
                 {}. supporting_votes: {supporting_precommits:?}",
                supporting_precommits.len(),
            );
            return Err(invalid_decision(msg));
        }
        Ok(ShcReturn::Decision(Decision { precommits: supporting_precommits, block }))
    }
}
