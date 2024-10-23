#[cfg(test)]
#[path = "single_height_consensus_test.rs"]
mod single_height_consensus_test;

use std::collections::hash_map::Entry;
use std::collections::{HashMap, VecDeque};
use std::time::Duration;

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
pub struct ShcTask {
    pub duration: Duration,
    pub event: StateMachineEvent,
}

#[derive(Debug, PartialEq)]
pub enum ShcReturn {
    Tasks(Vec<ShcTask>),
    Decision(Decision),
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

    /// Receive a proposal from a peer node. Returns only once the proposal has been fully received
    /// and processed.
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

        let block_receiver = context
            .validate_proposal(self.height, self.timeouts.proposal_timeout, p2p_messages_receiver)
            .await;

        let block = match block_receiver.await {
            Ok(block) => block,
            // ProposalFin never received from peer.
            Err(_) => {
                proposal_entry.insert(None);
                return self.process_inbound_proposal(context, &init, None).await;
            }
        };

        let fin = match fin_receiver.await {
            Ok(fin) => fin,
            // ProposalFin never received from peer.
            Err(_) => {
                proposal_entry.insert(None);
                return self.process_inbound_proposal(context, &init, None).await;
            }
        };
        // TODO(matan): Switch to signature validation.
        if block != fin {
            proposal_entry.insert(None);
            return self.process_inbound_proposal(context, &init, None).await;
        }
        proposal_entry.insert(Some(block));
        self.process_inbound_proposal(context, &init, Some(block)).await
    }

    async fn process_inbound_proposal<ContextT: ConsensusContext>(
        &mut self,
        context: &mut ContextT,
        init: &ProposalInit,
        proposal_id: Option<ProposalContentId>,
    ) -> Result<ShcReturn, ConsensusError> {
        let sm_proposal = StateMachineEvent::Proposal(proposal_id, init.round, init.valid_round);
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
        event: StateMachineEvent,
    ) -> Result<ShcReturn, ConsensusError> {
        debug!("Received Event: {:?}", event);
        match event {
            StateMachineEvent::TimeoutPropose(_)
            | StateMachineEvent::TimeoutPrevote(_)
            | StateMachineEvent::TimeoutPrecommit(_) => {
                let leader_fn =
                    |round: Round| -> ValidatorId { context.proposer(self.height, round) };
                let sm_events = self.state_machine.handle_event(event, &leader_fn);
                self.handle_state_machine_events(context, sm_events).await
            }
            StateMachineEvent::Prevote(proposal_id, round) => {
                let Some(last_vote) = &self.last_prevote else {
                    return Err(ConsensusError::InvalidEvent("No prevote to send".to_string()));
                };
                if last_vote.round > round {
                    return Ok(ShcReturn::Tasks(Vec::new()));
                }
                context.broadcast(ConsensusMessage::Vote(last_vote.clone())).await?;
                Ok(ShcReturn::Tasks(vec![ShcTask {
                    duration: self.timeouts.prevote_timeout,
                    event: StateMachineEvent::Prevote(proposal_id, round),
                }]))
            }
            StateMachineEvent::Precommit(proposal_id, round) => {
                let Some(last_vote) = &self.last_precommit else {
                    return Err(ConsensusError::InvalidEvent("No precommit to send".to_string()));
                };
                if last_vote.round > round {
                    return Ok(ShcReturn::Tasks(Vec::new()));
                }
                context.broadcast(ConsensusMessage::Vote(last_vote.clone())).await?;
                Ok(ShcReturn::Tasks(vec![ShcTask {
                    duration: self.timeouts.precommit_timeout,
                    event: StateMachineEvent::Precommit(proposal_id, round),
                }]))
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
                    events.append(
                        &mut self
                            .handle_state_machine_get_proposal(context, proposal_id, round)
                            .await,
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
                    ret_val.push(ShcTask { duration: self.timeouts.proposal_timeout, event });
                }
                StateMachineEvent::TimeoutPrevote(_) => {
                    ret_val.push(ShcTask { duration: self.timeouts.prevote_timeout, event });
                }
                StateMachineEvent::TimeoutPrecommit(_) => {
                    ret_val.push(ShcTask { duration: self.timeouts.precommit_timeout, event });
                }
            }
        }
        Ok(ShcReturn::Tasks(ret_val))
    }

    #[instrument(skip(self, context), level = "debug")]
    async fn handle_state_machine_get_proposal<ContextT: ConsensusContext>(
        &mut self,
        context: &mut ContextT,
        proposal_id: Option<ProposalContentId>,
        round: Round,
    ) -> VecDeque<StateMachineEvent> {
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
        let block = fin_receiver.await.expect("Block building failed.");
        let old = self.proposals.insert(round, Some(block));
        assert!(old.is_none(), "There should be no entry for this round.");
        let leader_fn = |round: Round| -> ValidatorId { context.proposer(self.height, round) };
        self.state_machine
            .handle_event(StateMachineEvent::GetProposal(Some(block), round), &leader_fn)
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
        let (votes, last_vote, duration, event) = match vote_type {
            VoteType::Prevote => (
                &mut self.prevotes,
                &mut self.last_prevote,
                self.timeouts.prevote_timeout,
                StateMachineEvent::Prevote(proposal_id, round),
            ),
            VoteType::Precommit => (
                &mut self.precommits,
                &mut self.last_precommit,
                self.timeouts.precommit_timeout,
                StateMachineEvent::Precommit(proposal_id, round),
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
        Ok(vec![ShcTask { duration, event }])
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
