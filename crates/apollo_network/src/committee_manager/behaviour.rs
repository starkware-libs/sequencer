use std::collections::VecDeque;
use std::convert::Infallible;
use std::task::{Context, Poll};

use futures::channel::{mpsc, oneshot};
use futures::StreamExt;
use libp2p::core::transport::PortUse;
use libp2p::core::Endpoint;
use libp2p::swarm::behaviour::ConnectionEstablished;
use libp2p::swarm::{
    dummy,
    CloseConnection,
    ConnectionClosed,
    ConnectionDenied,
    ConnectionId,
    DialFailure,
    FromSwarm,
    NetworkBehaviour,
    ToSwarm,
};
use libp2p::{Multiaddr, PeerId};
use tracing::{debug, warn};

use super::store::CommitteeStore;
use super::types::{CommitteeMember, EpochId, StakerId};
use crate::committee_manager::pending_mappings::PendingMappings;
use crate::mixed_behaviour::{BridgedBehaviour, ToOtherBehaviourEvent};

type AddCommitteeSender = mpsc::Sender<(EpochId, Vec<CommitteeMember>)>;
type AddCommitteeReceiver = mpsc::Receiver<(EpochId, Vec<CommitteeMember>)>;
pub type AddPeerRequest = (StakerId, PeerId, oneshot::Sender<bool>);
pub type AddPeerSender = mpsc::Sender<AddPeerRequest>;
type AddPeerReceiver = mpsc::Receiver<AddPeerRequest>;

pub struct CommitteeManagerHandles {
    /// Channel for Consensus to register new committees.
    pub add_committee_sender: AddCommitteeSender,
    /// Channel for Authenticator to request staker-to-peer mappings. Each request includes a
    /// oneshot sender for the committee manager to respond with `true` (added) or `false`
    /// (invalid staker or already mapped).
    pub add_peer_sender: AddPeerSender,
}

/// A libp2p `NetworkBehaviour` that exclusively owns the [`CommitteeStore`].
///
/// Responsibilities:
/// - Drains channel messages from Consensus (`AddCommittee`) and Authenticator (`AddPeerForStaker`)
///   in its `poll()` method, responding to the Authenticator via oneshot channels.
/// - Validates staker-to-peer mappings during authentication but defers adding them until
///   `ConnectionEstablished` fires, ensuring mappings only exist for fully established connections.
/// - Reacts to `ConnectionClosed` events from the swarm to clean up staker-to-peer mappings.
/// - Reacts to `DialFailure` events to clean up pending mappings for failed connections.
/// - Emits `ToSwarm::CloseConnection` for peers whose stakers were evicted when old epochs are
///   removed.
pub struct CommitteeManagerBehaviour {
    store: CommitteeStore,
    add_committee_receiver: AddCommitteeReceiver,
    add_peer_receiver: AddPeerReceiver,
    /// Peers that need to be disconnected due to epoch eviction.
    pending_disconnects: VecDeque<PeerId>,
    /// Pending staker-to-peer mappings: peer_id -> staker_id.
    /// These are validated during authentication but only added when ConnectionEstablished fires.
    pending_mappings: PendingMappings,
}
impl CommitteeManagerBehaviour {
    pub fn new(num_active_epochs: usize) -> (Self, CommitteeManagerHandles) {
        let store = CommitteeStore::new(num_active_epochs);
        let (add_committee_sender, add_committee_receiver) = mpsc::channel(100);
        let (add_peer_sender, add_peer_receiver) = mpsc::channel(100);
        let handles = CommitteeManagerHandles { add_committee_sender, add_peer_sender };
        (
            Self {
                store,
                add_committee_receiver,
                add_peer_receiver,
                pending_disconnects: VecDeque::new(),
                pending_mappings: PendingMappings::new(300),
            },
            handles,
        )
    }

    /// Compute the committee id as the Poseidon hash of the ordered member list.
    ///
    /// Each member contributes two field elements: `(staker_id, weight)`. The members are hashed
    /// in the order provided by consensus (no sorting).
    fn compute_committee_id(members: &[CommitteeMember]) -> super::types::CommitteeId {
        use starknet_types_core::felt::Felt;
        use starknet_types_core::hash::{Poseidon, StarkHash};

        let felts: Vec<Felt> =
            members.iter().flat_map(|m| [*m.staker_id.0.key(), Felt::from(m.weight.0)]).collect();

        Poseidon::hash_array(&felts)
    }

    /// Drain the `add_committee` channel and apply writes to the store.
    /// Any peers that need disconnecting due to epoch eviction are queued.
    fn process_add_committee_messages(&mut self, cx: &mut Context<'_>) {
        while let Poll::Ready(Some((epoch_id, members))) =
            self.add_committee_receiver.poll_next_unpin(cx)
        {
            let committee_id = Self::compute_committee_id(&members);
            match self.store.add_committee(epoch_id, committee_id, members) {
                Ok(peers_to_disconnect) => {
                    debug!("Added committee for epoch {epoch_id} with id {committee_id:?}");
                    for peer_id in peers_to_disconnect {
                        debug!(
                            "Queueing disconnect for peer {peer_id} (staker evicted from store)"
                        );
                        self.pending_disconnects.push_back(peer_id);
                    }
                }
                Err(e) => {
                    warn!("Failed to add committee for epoch {epoch_id}: {e}");
                }
            }
        }
    }

    /// Drain the `add_peer` channel, validate the staker, and store as pending mapping.
    /// The mapping will be added when ConnectionEstablished fires, or immediately if the
    /// connection is already established (handling race condition).
    fn process_add_peer_messages(&mut self, cx: &mut Context<'_>) {
        while let Poll::Ready(Some((staker_id, peer_id, response_tx))) =
            self.add_peer_receiver.poll_next_unpin(cx)
        {
            match self.store.can_add_peer_for_staker(&staker_id) {
                Ok(()) => {
                    // Connection not yet established - store as pending mapping.
                    self.pending_mappings.add_pending_connection(peer_id, staker_id);
                    debug!(
                        "Validated staker {staker_id:?} for peer {peer_id}, pending connection \
                         establishment"
                    );
                    let _ = response_tx.send(true);
                }
                Err(e) => {
                    warn!("Failed to validate staker {staker_id:?} for peer {peer_id}: {e}");
                    let _ = response_tx.send(false);
                }
            }
        }
    }

    /// Add a pending mapping to the store when connection is established.
    /// Also marks the connection as established to handle race conditions.
    fn pending_mapping_established(&mut self, peer_id: &PeerId) {
        if let Some(staker_id) = self.pending_mappings.pending_connection_established(peer_id) {
            // If there's a pending mapping, add it now.
            match self.store.add_peer_for_staker(staker_id, *peer_id) {
                Ok(()) => debug!(
                    "Mapped staker {staker_id:?} to peer {peer_id} in committee store (connection \
                     established)"
                ),
                Err(e) => warn!(
                    "Failed to map staker {staker_id:?} to peer {peer_id} on connection \
                     establishment: {e}"
                ),
            }

            self.pending_mappings.remove_pending_peers(&staker_id);
        }
    }

    /// Clean up a pending mapping when connection fails.
    fn remove_pending_mapping(&mut self, peer_id: &PeerId) {
        self.pending_mappings.remove_pending_peer(peer_id);
    }
}

impl NetworkBehaviour for CommitteeManagerBehaviour {
    type ConnectionHandler = dummy::ConnectionHandler;
    type ToSwarm = Infallible;

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        Ok(dummy::ConnectionHandler)
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
        _port_use: PortUse,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        Ok(dummy::ConnectionHandler)
    }

    fn on_connection_handler_event(
        &mut self,
        _peer_id: PeerId,
        _connection_id: ConnectionId,
        _event: libp2p::swarm::THandlerOutEvent<Self>,
    ) {
    }

    fn on_swarm_event(&mut self, event: FromSwarm<'_>) {
        match event {
            FromSwarm::ConnectionEstablished(ConnectionEstablished { peer_id, .. }) => {
                // Connection fully established - add the pending mapping if it exists.
                self.pending_mapping_established(&peer_id);
            }
            FromSwarm::ConnectionClosed(ConnectionClosed { peer_id, .. }) => {
                // Clean up both active and pending mappings
                self.store.remove_peer(&peer_id);
                self.pending_mappings.remove_pending_peer(&peer_id);
                debug!("Removed peer {peer_id} from committee store on connection close");
            }
            FromSwarm::DialFailure(DialFailure { peer_id: Some(peer_id), .. }) => {
                // Connection failed before establishment - clean up pending mapping.
                self.remove_pending_mapping(&peer_id);
            }
            _ => {}
        }
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<
        ToSwarm<
            Self::ToSwarm,
            <Self::ConnectionHandler as libp2p::swarm::ConnectionHandler>::FromBehaviour,
        >,
    > {
        // Drain pending disconnects first (from previous epoch evictions).
        if let Some(peer_id) = self.pending_disconnects.pop_front() {
            return Poll::Ready(ToSwarm::CloseConnection {
                peer_id,
                connection: CloseConnection::All,
            });
        }

        self.process_add_committee_messages(cx);
        self.process_add_peer_messages(cx);

        // Emit any disconnects that were queued during message processing.
        if let Some(peer_id) = self.pending_disconnects.pop_front() {
            return Poll::Ready(ToSwarm::CloseConnection {
                peer_id,
                connection: CloseConnection::All,
            });
        }

        Poll::Pending
    }
}

impl BridgedBehaviour for CommitteeManagerBehaviour {
    fn on_other_behaviour_event(&mut self, _event: &ToOtherBehaviourEvent) {}
}
