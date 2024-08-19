#[cfg(test)]
#[path = "single_height_consensus_test.rs"]
mod single_height_consensus_test;

use std::collections::hash_map::Entry;
use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use futures::channel::{mpsc, oneshot};
use papyrus_protobuf::consensus::{ConsensusMessage, Vote, VoteType};
use starknet_api::block::{BlockHash, BlockNumber};
use tracing::{debug, info, instrument, trace, warn};

use crate::config::TimeoutsConfig;
use crate::state_machine::{StateMachine, StateMachineEvent};
use crate::types::{
    ConsensusBlock,
    ConsensusContext,
    ConsensusError,
    Decision,
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
pub enum ShcReturn<BlockT: ConsensusBlock> {
    Tasks(Vec<ShcTask>),
    Decision(Decision<BlockT>),
}

/// Struct which represents a single height of consensus. Each height is expected to be begun with a
/// call to `start`, which is relevant if we are the proposer for this height's first round.
/// SingleHeightConsensus receives messages directly as parameters to function calls. It can send
/// out messages "directly" to the network, and returning a decision to the caller.
pub(crate) struct SingleHeightConsensus<BlockT: ConsensusBlock> {
    height: BlockNumber,
    validators: Vec<ValidatorId>,
    id: ValidatorId,
    timeouts: TimeoutsConfig,
    state_machine: StateMachine,
    proposals: HashMap<Round, Option<BlockT>>,
    prevotes: HashMap<(Round, ValidatorId), Vote>,
    precommits: HashMap<(Round, ValidatorId), Vote>,
}

impl<BlockT: ConsensusBlock> SingleHeightConsensus<BlockT> {
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
        }
    }

    #[instrument(skip_all, fields(height=self.height.0), level = "debug")]
    pub(crate) async fn start<ContextT: ConsensusContext<Block = BlockT>>(
        &mut self,
        context: &mut ContextT,
    ) -> Result<ShcReturn<BlockT>, ConsensusError> {
        info!("Starting consensus with validators {:?}", self.validators);
        let leader_fn =
            |_round: Round| -> ValidatorId { context.proposer(&self.validators, self.height) };
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
    pub(crate) async fn handle_proposal<ContextT: ConsensusContext<Block = BlockT>>(
        &mut self,
        context: &mut ContextT,
        init: ProposalInit,
        p2p_messages_receiver: mpsc::Receiver<<BlockT as ConsensusBlock>::ProposalChunk>,
        fin_receiver: oneshot::Receiver<BlockHash>,
    ) -> Result<ShcReturn<BlockT>, ConsensusError> {
        debug!(
            "Received proposal: height={}, round={}, proposer={:?}",
            init.height.0, init.round, init.proposer
        );
        let proposer_id = context.proposer(&self.validators, self.height);
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
            return Ok(ShcReturn::Tasks(vec![]));
        };

        let block_receiver = context.validate_proposal(self.height, p2p_messages_receiver).await;

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
        let block_id = block.id();
        if block_id != fin {
            proposal_entry.insert(None);
            return self.process_inbound_proposal(context, &init, None).await;
        }
        proposal_entry.insert(Some(block));
        self.process_inbound_proposal(context, &init, Some(block_id)).await
    }

    async fn process_inbound_proposal<ContextT: ConsensusContext<Block = BlockT>>(
        &mut self,
        context: &mut ContextT,
        init: &ProposalInit,
        block_id: Option<BlockHash>,
    ) -> Result<ShcReturn<BlockT>, ConsensusError> {
        let sm_proposal = StateMachineEvent::Proposal(block_id, init.round);
        let leader_fn =
            |_round: Round| -> ValidatorId { context.proposer(&self.validators, self.height) };
        let sm_events = self.state_machine.handle_event(sm_proposal, &leader_fn);
        self.handle_state_machine_events(context, sm_events).await
    }

    /// Handle messages from peer nodes.
    #[instrument(skip_all)]
    pub(crate) async fn handle_message<ContextT: ConsensusContext<Block = BlockT>>(
        &mut self,
        context: &mut ContextT,
        message: ConsensusMessage,
    ) -> Result<ShcReturn<BlockT>, ConsensusError> {
        debug!("Received message: {:?}", message);
        match message {
            ConsensusMessage::Proposal(_) => {
                unimplemented!("Proposals should use `handle_proposal` due to fake streaming")
            }
            ConsensusMessage::Vote(vote) => self.handle_vote(context, vote).await,
        }
    }

    pub async fn handle_task<ContextT: ConsensusContext<Block = BlockT>>(
        &mut self,
        context: &mut ContextT,
        task: ShcTask,
    ) -> Result<ShcReturn<BlockT>, ConsensusError> {
        debug!("Received task: {:?}", task);
        match task.event {
            StateMachineEvent::TimeoutPropose(_)
            | StateMachineEvent::TimeoutPrevote(_)
            | StateMachineEvent::TimeoutPrecommit(_) => {
                let leader_fn = |_round: Round| -> ValidatorId {
                    context.proposer(&self.validators, self.height)
                };
                let sm_events = self.state_machine.handle_event(task.event, &leader_fn);
                self.handle_state_machine_events(context, sm_events).await
            }
            _ => unimplemented!("Unexpected task: {:?}", task),
        }
    }

    #[instrument(skip_all)]
    async fn handle_vote<ContextT: ConsensusContext<Block = BlockT>>(
        &mut self,
        context: &mut ContextT,
        vote: Vote,
    ) -> Result<ShcReturn<BlockT>, ConsensusError> {
        if !self.validators.contains(&vote.voter) {
            return Err(ConsensusError::InvalidVote(
                vote.clone(),
                format!("voter {:?} not in validators {:?}", vote.voter, self.validators),
            ));
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
                    return Ok(ShcReturn::Tasks(vec![]));
                }
            }
        }
        let leader_fn =
            |_round: Round| -> ValidatorId { context.proposer(&self.validators, self.height) };
        let sm_events = self.state_machine.handle_event(sm_vote, &leader_fn);
        self.handle_state_machine_events(context, sm_events).await
    }

    // Handle events output by the state machine.
    #[instrument(skip_all)]
    async fn handle_state_machine_events<ContextT: ConsensusContext<Block = BlockT>>(
        &mut self,
        context: &mut ContextT,
        mut events: VecDeque<StateMachineEvent>,
    ) -> Result<ShcReturn<BlockT>, ConsensusError> {
        let mut ret_val = vec![];
        while let Some(event) = events.pop_front() {
            trace!("Handling event: {:?}", event);
            match event {
                StateMachineEvent::GetProposal(block_hash, round) => {
                    events.append(
                        &mut self
                            .handle_state_machine_get_proposal(context, block_hash, round)
                            .await,
                    );
                }
                StateMachineEvent::Proposal(_, _) => {
                    // Ignore proposals sent by the StateMachine as SingleHeightConsensus already
                    // sent this out when responding to a GetProposal.
                    // TODO(matan): How do we handle this when validValue is set?
                }
                StateMachineEvent::Decision(block_hash, round) => {
                    return self.handle_state_machine_decision(block_hash, round).await;
                }
                StateMachineEvent::Prevote(block_hash, round) => {
                    self.handle_state_machine_vote(context, block_hash, round, VoteType::Prevote)
                        .await?;
                }
                StateMachineEvent::Precommit(block_hash, round) => {
                    self.handle_state_machine_vote(context, block_hash, round, VoteType::Precommit)
                        .await?;
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
    async fn handle_state_machine_get_proposal<ContextT: ConsensusContext<Block = BlockT>>(
        &mut self,
        context: &mut ContextT,
        block_hash: Option<BlockHash>,
        round: Round,
    ) -> VecDeque<StateMachineEvent> {
        assert!(
            block_hash.is_none(),
            "BlockHash must be None since the state machine is requesting a BlockHash"
        );
        debug!("Proposer");

        let (p2p_messages_receiver, block_receiver) = context.build_proposal(self.height).await;
        let (fin_sender, fin_receiver) = oneshot::channel();
        let init = ProposalInit { height: self.height, round, proposer: self.id };
        // Peering is a permanent component, so if sending to it fails we cannot continue.
        context
            .propose(init, p2p_messages_receiver, fin_receiver)
            .await
            .expect("Failed sending Proposal to Peering");
        let block = block_receiver.await.expect("Block building failed.");
        let id = block.id();
        // If we choose to ignore this error, we should carefully consider how this affects
        // Tendermint. The partially synchronous model assumes all messages arrive at some point,
        // and this failure means this proposal will never arrive.
        //
        // TODO(matan): Switch this to the Proposal signature.
        fin_sender.send(id).expect("Failed to send ProposalFin to Peering.");
        let old = self.proposals.insert(round, Some(block));
        assert!(old.is_none(), "There should be no entry for this round.");
        let leader_fn =
            |_round: Round| -> ValidatorId { context.proposer(&self.validators, self.height) };
        self.state_machine.handle_event(StateMachineEvent::GetProposal(Some(id), round), &leader_fn)
    }

    #[instrument(skip_all)]
    async fn handle_state_machine_vote<ContextT: ConsensusContext<Block = BlockT>>(
        &mut self,
        context: &mut ContextT,
        block_hash: Option<BlockHash>,
        round: Round,
        vote_type: VoteType,
    ) -> Result<ShcReturn<BlockT>, ConsensusError> {
        let votes = match vote_type {
            VoteType::Prevote => &mut self.prevotes,
            VoteType::Precommit => &mut self.precommits,
        };
        let vote = Vote { vote_type, height: self.height.0, round, block_hash, voter: self.id };
        if let Some(old) = votes.insert((round, self.id), vote.clone()) {
            // TODO(matan): Consider refactoring not to panic, rather log and return the error.
            panic!("State machine should not send repeat votes: old={:?}, new={:?}", old, vote);
        }
        context.broadcast(ConsensusMessage::Vote(vote)).await?;
        Ok(ShcReturn::Tasks(vec![]))
    }

    #[instrument(skip_all)]
    async fn handle_state_machine_decision(
        &mut self,
        block_hash: BlockHash,
        round: Round,
    ) -> Result<ShcReturn<BlockT>, ConsensusError> {
        let block = self
            .proposals
            .remove(&round)
            .expect("StateMachine arrived at an unknown decision")
            .expect("StateMachine should not decide on a missing proposal");
        assert_eq!(block.id(), block_hash, "StateMachine block hash should match the stored block");
        let supporting_precommits: Vec<Vote> = self
            .validators
            .iter()
            .filter_map(|v| {
                let vote = self.precommits.get(&(round, *v))?;
                if vote.block_hash != Some(block_hash) {
                    return None;
                }
                Some(vote.clone())
            })
            .collect();
        // TODO(matan): Check actual weights.
        assert!(supporting_precommits.len() >= self.state_machine.quorum_size() as usize);
        Ok(ShcReturn::Decision(Decision { precommits: supporting_precommits, block }))
    }
}
