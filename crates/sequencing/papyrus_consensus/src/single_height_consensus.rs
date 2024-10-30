#[cfg(test)]
#[path = "single_height_consensus_test.rs"]
mod single_height_consensus_test;

use std::collections::hash_map::Entry;
use std::collections::{HashMap, VecDeque};
use std::time::Duration;

#[cfg(test)]
use enum_as_inner::EnumAsInner;
use futures::channel::{mpsc, oneshot};
use papyrus_protobuf::consensus::{ConsensusMessage, Vote, VoteType};
use starknet_api::block::BlockNumber;
use tracing::{debug, info, instrument, trace, warn};

use crate::config::TimeoutsConfig;
use crate::state_machine::{StateMachine, StateMachineEvent};
use crate::types::{
    ConsensusContext,
    ConsensusError,
    Decision,
    ProposalContentId,
    ProposalInit,
    Round,
    ValidatorId,
};

#[derive(Debug, PartialEq)]
#[cfg_attr(test, derive(EnumAsInner))]
pub enum ShcReturn {
    Tasks(Vec<ShcTask>),
    Decision(Decision),
}

#[derive(Debug, Clone)]
pub enum ShcEvent {
    TimeoutPropose(StateMachineEvent),
    TimeoutPrevote(StateMachineEvent),
    TimeoutPrecommit(StateMachineEvent),
    Prevote(StateMachineEvent),
    Precommit(StateMachineEvent),
    BuildProposal(StateMachineEvent),
    // TODO: Replace ProposalContentId with the unvalidated signature from the proposer.
    ValidateProposal(StateMachineEvent, Option<ProposalContentId>),
}

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
    BuildProposal(Round, oneshot::Receiver<ProposalContentId>),
    /// Validating a proposal is handled in 3 stages:
    /// 1. The SHC validates `ProposalInit`, then starts block validation within the context.
    /// 2. SHC returns, allowing the context to validate the content while the Manager await the
    ///    result without blocking consensus.
    /// 3. Once validation is complete, the manager returns the built proposal to the SHC as an
    ///    event, which can be sent to the SM.
    ValidateProposal(
        ProposalInit,
        oneshot::Receiver<ProposalContentId>, // Block built from the content.
        oneshot::Receiver<ProposalContentId>, // Fin sent by the proposer.
    ),
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
            (ShcTask::ValidateProposal(pi1, _, _), ShcTask::ValidateProposal(pi2, _, _)) => {
                pi1 == pi2
            }
            _ => false,
        }
    }
}

impl ShcTask {
    pub async fn run(self) -> ShcEvent {
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
                let proposal_id = receiver.await.expect("Block building failed.");
                ShcEvent::BuildProposal(StateMachineEvent::GetProposal(Some(proposal_id), round))
            }
            ShcTask::ValidateProposal(
                init,
                id_built_from_content_receiver,
                fin_from_proposer_receiver,
            ) => {
                let proposal_id = match id_built_from_content_receiver.await {
                    Ok(proposal_id) => Some(proposal_id),
                    // Proposal never received from peer.
                    Err(_) => None,
                };
                let fin = match fin_from_proposer_receiver.await {
                    Ok(fin) => Some(fin),
                    // ProposalFin never received from peer.
                    Err(_) => None,
                };
                ShcEvent::ValidateProposal(
                    StateMachineEvent::Proposal(proposal_id, init.round, init.valid_round),
                    fin,
                )
            }
        }
    }
}

/// Struct which represents a single height of consensus. Each height is expected to be begun with a
/// call to `start`, which is relevant if we are the proposer for this height's first round.
/// SingleHeightConsensus receives messages directly as parameters to function calls. It can send
/// out messages "directly" to the network, and returning a decision to the caller.
pub(crate) struct SingleHeightConsensus {
    height: BlockNumber,
    validators: Vec<ValidatorId>,
    id: ValidatorId,
    timeouts: TimeoutsConfig,
    state_machine: StateMachine,
    proposals: HashMap<Round, Option<ProposalContentId>>,
    prevotes: HashMap<(Round, ValidatorId), Vote>,
    precommits: HashMap<(Round, ValidatorId), Vote>,
    last_prevote: Option<Vote>,
    last_precommit: Option<Vote>,
}

impl SingleHeightConsensus {
    pub(crate) fn new(
        height: BlockNumber,
        id: ValidatorId,
        validators: Vec<ValidatorId>,
        timeouts: TimeoutsConfig,
    ) -> Self {
        // TODO(matan): Use actual weights, not just `len`.
        let state_machine = StateMachine::new(id, validators.len() as u32);
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

    #[instrument(skip_all, fields(height=self.height.0), level = "debug")]
    pub(crate) async fn start<ContextT: ConsensusContext>(
        &mut self,
        context: &mut ContextT,
    ) -> Result<ShcReturn, ConsensusError> {
        info!("Starting consensus with validators {:?}", self.validators);
        let leader_fn = |round: Round| -> ValidatorId { context.proposer(self.height, round) };
        let events = self.state_machine.start(&leader_fn);
        self.handle_state_machine_events(context, events).await
    }

    /// Process the proposal init and initiate block validation. See [`ShcTask::ValidateProposal`]
    /// for more details on the full proposal flow.
    #[instrument(
        skip_all,
        fields(height = %self.height),
        level = "debug",
    )]
    pub(crate) async fn handle_proposal<ContextT: ConsensusContext>(
        &mut self,
        context: &mut ContextT,
        init: ProposalInit,
        p2p_messages_receiver: mpsc::Receiver<ContextT::ProposalChunk>,
        fin_receiver: oneshot::Receiver<ProposalContentId>,
    ) -> Result<ShcReturn, ConsensusError> {
        debug!(
            "Received proposal: height={}, round={}, proposer={:?}",
            init.height.0, init.round, init.proposer
        );
        let proposer_id = context.proposer(self.height, init.round);
        if init.height != self.height {
            let msg = format!("invalid height: expected {:?}, got {:?}", self.height, init.height);
            return Err(ConsensusError::InvalidProposal(proposer_id, self.height, msg));
        }
        if init.proposer != proposer_id {
            let msg =
                format!("invalid proposer: expected {:?}, got {:?}", proposer_id, init.proposer);
            return Err(ConsensusError::InvalidProposal(proposer_id, self.height, msg));
        }
        let Entry::Vacant(proposal_entry) = self.proposals.entry(init.round) else {
            warn!("Round {} already has a proposal, ignoring", init.round);
            return Ok(ShcReturn::Tasks(Vec::new()));
        };
        // Since validating the proposal is non-blocking, we want to avoid validating the same round
        // twice in parallel. This could be caused by a network repeat or a malicious spam attack.
        proposal_entry.insert(None);
        let block_receiver = context
            .validate_proposal(self.height, self.timeouts.proposal_timeout, p2p_messages_receiver)
            .await;
        Ok(ShcReturn::Tasks(vec![ShcTask::ValidateProposal(init, block_receiver, fin_receiver)]))
    }

    async fn process_inbound_proposal<ContextT: ConsensusContext>(
        &mut self,
        context: &mut ContextT,
        sm_proposal: StateMachineEvent,
    ) -> Result<ShcReturn, ConsensusError> {
        let leader_fn = |round: Round| -> ValidatorId { context.proposer(self.height, round) };
        let sm_events = self.state_machine.handle_event(sm_proposal, &leader_fn);
        self.handle_state_machine_events(context, sm_events).await
    }

    /// Handle messages from peer nodes.
    #[instrument(skip_all)]
    pub(crate) async fn handle_message<ContextT: ConsensusContext>(
        &mut self,
        context: &mut ContextT,
        message: ConsensusMessage,
    ) -> Result<ShcReturn, ConsensusError> {
        debug!("Received message: {:?}", message);
        match message {
            ConsensusMessage::Proposal(_) => {
                unimplemented!("Proposals should use `handle_proposal` due to fake streaming")
            }
            ConsensusMessage::Vote(vote) => self.handle_vote(context, vote).await,
        }
    }

    pub async fn handle_event<ContextT: ConsensusContext>(
        &mut self,
        context: &mut ContextT,
        event: ShcEvent,
    ) -> Result<ShcReturn, ConsensusError> {
        debug!("Received ShcEvent: {:?}", event);
        match event {
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
                    return Err(ConsensusError::InvalidEvent("No prevote to send".to_string()));
                };
                if last_vote.round > round {
                    return Ok(ShcReturn::Tasks(Vec::new()));
                }
                context.broadcast(ConsensusMessage::Vote(last_vote.clone())).await?;
                Ok(ShcReturn::Tasks(vec![ShcTask::Prevote(
                    self.timeouts.prevote_timeout,
                    StateMachineEvent::Prevote(proposal_id, round),
                )]))
            }
            ShcEvent::Precommit(StateMachineEvent::Precommit(proposal_id, round)) => {
                let Some(last_vote) = &self.last_precommit else {
                    return Err(ConsensusError::InvalidEvent("No precommit to send".to_string()));
                };
                if last_vote.round > round {
                    return Ok(ShcReturn::Tasks(Vec::new()));
                }
                context.broadcast(ConsensusMessage::Vote(last_vote.clone())).await?;
                Ok(ShcReturn::Tasks(vec![ShcTask::Precommit(
                    self.timeouts.precommit_timeout,
                    StateMachineEvent::Precommit(proposal_id, round),
                )]))
            }
            ShcEvent::ValidateProposal(
                StateMachineEvent::Proposal(proposal_id, round, valid_round),
                fin,
            ) => {
                // TODO(matan): Switch to signature validation.
                let id = if proposal_id != fin {
                    warn!(
                        "unexpected, possible due to network issue: proposal_id={:#064x?}, \
                         fin={:#064x?}",
                        proposal_id, fin
                    );
                    None
                } else {
                    proposal_id
                };
                // Retaining the entry for this round prevents us from receiving another proposal on
                // this round. If the validations failed, which can be caused by a network issue, we
                // may want to re-open ourselves to this round. The downside is that this may open
                // us to a spam attack.
                // TODO(Asmaa): discuss solution to this issue.
                self.proposals.insert(round, id);
                self.process_inbound_proposal(
                    context,
                    StateMachineEvent::Proposal(id, round, valid_round),
                )
                .await
            }
            ShcEvent::BuildProposal(StateMachineEvent::GetProposal(proposal_id, round)) => {
                let old = self.proposals.insert(round, proposal_id);
                assert!(old.is_none(), "There should be no entry for this round.");
                let leader_fn =
                    |round: Round| -> ValidatorId { context.proposer(self.height, round) };
                let sm_events = self
                    .state_machine
                    .handle_event(StateMachineEvent::GetProposal(proposal_id, round), &leader_fn);
                self.handle_state_machine_events(context, sm_events).await
            }
            _ => unimplemented!("Unexpected event: {:?}", event),
        }
    }

    #[instrument(skip_all)]
    async fn handle_vote<ContextT: ConsensusContext>(
        &mut self,
        context: &mut ContextT,
        vote: Vote,
    ) -> Result<ShcReturn, ConsensusError> {
        if !self.validators.contains(&vote.voter) {
            debug!("Ignoring vote from voter not in validators: vote={:?}", vote);
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
                    return Err(ConsensusError::Equivocation(
                        self.height,
                        ConsensusMessage::Vote(old.clone()),
                        ConsensusMessage::Vote(vote),
                    ));
                } else {
                    // Replay, ignore.
                    return Ok(ShcReturn::Tasks(Vec::new()));
                }
            }
        }
        let leader_fn = |round: Round| -> ValidatorId { context.proposer(self.height, round) };
        let sm_events = self.state_machine.handle_event(sm_vote, &leader_fn);
        self.handle_state_machine_events(context, sm_events).await
    }

    // Handle events output by the state machine.
    #[instrument(skip_all)]
    async fn handle_state_machine_events<ContextT: ConsensusContext>(
        &mut self,
        context: &mut ContextT,
        mut events: VecDeque<StateMachineEvent>,
    ) -> Result<ShcReturn, ConsensusError> {
        let mut ret_val = Vec::new();
        while let Some(event) = events.pop_front() {
            trace!("Handling event: {:?}", event);
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
    #[instrument(skip(self, context), level = "debug")]
    async fn handle_state_machine_get_proposal<ContextT: ConsensusContext>(
        &mut self,
        context: &mut ContextT,
        proposal_id: Option<ProposalContentId>,
        round: Round,
    ) -> Vec<ShcTask> {
        assert!(
            proposal_id.is_none(),
            "ProposalContentId must be None since the state machine is requesting a \
             ProposalContentId"
        );
        debug!("Proposer");

        // TODO: Figure out how to handle failed proposal building. I believe this should be handled
        // by applying timeoutPropose when we are the leader.
        let init =
            ProposalInit { height: self.height, round, proposer: self.id, valid_round: None };
        let fin_receiver = context.build_proposal(init, self.timeouts.proposal_timeout).await;
        vec![ShcTask::BuildProposal(round, fin_receiver)]
    }

    #[instrument(skip(self, context), level = "debug")]
    async fn handle_state_machine_proposal<ContextT: ConsensusContext>(
        &mut self,
        context: &mut ContextT,
        proposal_id: Option<ProposalContentId>,
        round: Round,
        valid_round: Option<Round>,
    ) {
        let proposal_id = proposal_id.expect("StateMachine should not propose a None proposal_id");
        let Some(valid_round) = valid_round else {
            // newly built so just streamed
            return;
        };
        let id = self
            .proposals
            .get(&valid_round)
            .expect("proposals should have proposal for valid_round")
            .expect("proposal should not be None");
        assert_eq!(id, proposal_id, "proposal should match the stored proposal");
        let init = ProposalInit {
            height: self.height,
            round,
            proposer: self.id,
            valid_round: Some(valid_round),
        };
        context.repropose(id, init).await;
        let old = self.proposals.insert(round, Some(proposal_id));
        assert!(old.is_none(), "There should be no entry for this round.");
    }

    #[instrument(skip_all)]
    async fn handle_state_machine_vote<ContextT: ConsensusContext>(
        &mut self,
        context: &mut ContextT,
        proposal_id: Option<ProposalContentId>,
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
            // TODO(matan): Consider refactoring not to panic, rather log and return the error.
            panic!("State machine should not send repeat votes: old={:?}, new={:?}", old, vote);
        }
        context.broadcast(ConsensusMessage::Vote(vote.clone())).await?;
        if last_vote.as_ref().map_or(false, |last| round < last.round) {
            return Ok(Vec::new());
        }
        *last_vote = Some(vote);
        Ok(vec![task])
    }

    #[instrument(skip_all)]
    async fn handle_state_machine_decision(
        &mut self,
        proposal_id: ProposalContentId,
        round: Round,
    ) -> Result<ShcReturn, ConsensusError> {
        let block = self
            .proposals
            .remove(&round)
            .expect("StateMachine arrived at an unknown decision")
            .expect("StateMachine should not decide on a missing proposal");
        assert_eq!(block, proposal_id, "StateMachine block hash should match the stored block");
        let supporting_precommits: Vec<Vote> = self
            .validators
            .iter()
            .filter_map(|v| {
                let vote = self.precommits.get(&(round, *v))?;
                if vote.block_hash == Some(proposal_id) { Some(vote.clone()) } else { None }
            })
            .collect();
        // TODO(matan): Check actual weights.
        assert!(supporting_precommits.len() >= self.state_machine.quorum_size() as usize);
        Ok(ShcReturn::Decision(Decision { precommits: supporting_precommits, block }))
    }
}
