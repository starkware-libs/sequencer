use std::collections::VecDeque;
use std::convert::Infallible;
use std::sync::{Arc, RwLock};
use std::task::{Context, Poll};

use futures::channel::mpsc;
use futures::StreamExt;
use libp2p::core::transport::PortUse;
use libp2p::core::Endpoint;
use libp2p::swarm::{
    dummy,
    CloseConnection,
    ConnectionClosed,
    ConnectionDenied,
    ConnectionId,
    FromSwarm,
    NetworkBehaviour,
    ToSwarm,
};
use libp2p::{Multiaddr, PeerId};
use tracing::{debug, warn};

use super::store::CommitteeStore;
use super::types::{CommitteeMember, EpochId, StakerId};
use crate::mixed_behaviour::{BridgedBehaviour, ToOtherBehaviourEvent};

type AddCommitteeSender = mpsc::Sender<(EpochId, Vec<CommitteeMember>)>;
type AddCommitteeReceiver = mpsc::Receiver<(EpochId, Vec<CommitteeMember>)>;
pub type StakerToPeerSender = mpsc::Sender<(StakerId, PeerId)>;
type StakerToPeerReceiver = mpsc::Receiver<(StakerId, PeerId)>;

pub struct CommitteeManagerHandles {
    /// Read handle to the committee store. Acquire a read lock to query committee/epoch data.
    pub store: Arc<RwLock<CommitteeStore>>,
    /// Channel for Consensus to register new committees.
    pub add_committee_sender: AddCommitteeSender,
    /// Channel for Authenticator to register staker-to-peer mappings after successful handshake.
    pub add_peer_sender: StakerToPeerSender,
}

/// A libp2p `NetworkBehaviour` that owns the write side of the [`CommitteeStore`].
///
/// Responsibilities:
/// - Drains channel messages from Consensus (`AddCommittee`) and Authenticator (`AddPeerForStaker`)
///   in its `poll()` method.
/// - Reacts to `ConnectionClosed` events from the swarm to clean up staker-to-peer mappings.
/// - Emits `ToSwarm::CloseConnection` for peers whose stakers were evicted when old epochs are
///   removed.
pub struct CommitteeManagerBehaviour {
    store: Arc<RwLock<CommitteeStore>>,
    add_committee_receiver: AddCommitteeReceiver,
    add_peer_receiver: StakerToPeerReceiver,
    /// Peers that need to be disconnected due to epoch eviction.
    pending_disconnects: VecDeque<PeerId>,
}
impl CommitteeManagerBehaviour {
    pub fn new(num_active_epochs: usize) -> (Self, CommitteeManagerHandles) {
        let store = Arc::new(RwLock::new(CommitteeStore::new(num_active_epochs)));
        let (add_committee_sender, add_committee_receiver) = mpsc::channel(100);
        let (add_peer_sender, add_peer_receiver) = mpsc::channel(100);
        let handles =
            CommitteeManagerHandles { store: store.clone(), add_committee_sender, add_peer_sender };
        (
            Self {
                store,
                add_committee_receiver,
                add_peer_receiver,
                pending_disconnects: VecDeque::new(),
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

        let felts: Vec<Felt> = members
            .iter()
            .flat_map(|m| {
                [*m.staker_id.0.key(), Felt::from(m.weight.0)]
            })
            .collect();

        Poseidon::hash_array(&felts)
    }

    /// Drain the `add_committee` channel and apply writes to the store.
    /// Any peers that need disconnecting due to epoch eviction are queued.
    fn process_add_committee_messages(&mut self, cx: &mut Context<'_>) {
        while let Poll::Ready(Some((epoch_id, members))) =
            self.add_committee_receiver.poll_next_unpin(cx)
        {
            let committee_id = Self::compute_committee_id(&members);
            let mut store = self.store.write().expect("CommitteeStore lock poisoned");
            match store.add_committee(epoch_id, committee_id, members) {
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

    /// Drain the `add_peer` channel and apply writes to the store.
    fn process_add_peer_messages(&mut self, cx: &mut Context<'_>) {
        while let Poll::Ready(Some((staker_id, peer_id))) =
            self.add_peer_receiver.poll_next_unpin(cx)
        {
            let mut store = self.store.write().expect("CommitteeStore lock poisoned");
            match store.add_peer_for_staker(staker_id, peer_id) {
                Ok(()) => {
                    debug!("Mapped staker {staker_id:?} to peer {peer_id} in committee store");
                }
                Err(e) => {
                    warn!("Failed to map staker {staker_id:?} to peer {peer_id}: {e}");
                }
            }
        }
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
        if let FromSwarm::ConnectionClosed(ConnectionClosed { peer_id, .. }) = event {
            let mut store = self.store.write().expect("CommitteeStore lock poisoned");
            store.remove_peer(&peer_id);
            debug!("Removed peer {peer_id} from committee store on connection close");
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
