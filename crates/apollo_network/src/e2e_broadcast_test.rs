use std::collections::HashSet;
use std::time::Duration;

use futures::{FutureExt, StreamExt};
use libp2p::core::multiaddr::Protocol;
use libp2p::swarm::SwarmEvent;
use libp2p::{Multiaddr, PeerId, Swarm};
use libp2p_swarm_test::SwarmExt;
use starknet_api::core::ChainId;

use crate::discovery::DiscoveryConfig;
use crate::gossipsub_impl::Topic;
use crate::mixed_behaviour::MixedBehaviour;
use crate::network_manager::{BroadcastTopicClientTrait, GenericNetworkManager};
use crate::peer_manager::PeerManagerConfig;
use crate::prune_dead_connections::{DEFAULT_PING_INTERVAL, DEFAULT_PING_TIMEOUT};
use crate::{sqmr, Bytes};

const TIMEOUT: Duration = Duration::from_secs(5);

fn create_swarm(bootstrap_peer_multiaddr: Option<Multiaddr>) -> Swarm<MixedBehaviour> {
    let mut swarm = Swarm::new_ephemeral_tokio(|keypair| {
        MixedBehaviour::new(
            sqmr::Config::default(),
            DiscoveryConfig::default(),
            PeerManagerConfig::default(),
            None,
            None,
            keypair.clone(),
            bootstrap_peer_multiaddr.map(|multiaddr| vec![multiaddr]),
            ChainId::Mainnet,
            None,
            DEFAULT_PING_INTERVAL,
            DEFAULT_PING_TIMEOUT,
        )
    });
    swarm.listen_on(Protocol::Memory(0).into()).unwrap();
    swarm
}

/// Poll the swarm to discover its listen address, then set it as an external address.
/// Only safe to call on swarms without bootstrap peers (no events will be lost).
async fn get_listen_address(swarm: &mut Swarm<MixedBehaviour>) -> Multiaddr {
    // Not using SwarmExt::listen because it panics if the swarm emits other events
    let address = swarm
        .wait(|event| match event {
            SwarmEvent::NewListenAddr { address, .. } => Some(address),
            _ => None,
        })
        .await;
    swarm.add_external_address(address.clone());
    address
}

const MESSAGE_METADATA_BUFFER_SIZE: usize = 100000;

fn create_network_manager(
    swarm: Swarm<MixedBehaviour>,
) -> GenericNetworkManager<Swarm<MixedBehaviour>> {
    GenericNetworkManager::generic_new(
        swarm,
        None,
        None,
        MESSAGE_METADATA_BUFFER_SIZE,
        MESSAGE_METADATA_BUFFER_SIZE,
    )
}

const BUFFER_SIZE: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Number(pub u8);

#[derive(Debug)]
struct EmptyBytesError;

impl TryFrom<Bytes> for Number {
    type Error = EmptyBytesError;

    fn try_from(bytes: Bytes) -> Result<Self, Self::Error> {
        bytes.first().map(|x| Number(*x)).ok_or(EmptyBytesError)
    }
}

impl From<Number> for Bytes {
    fn from(num: Number) -> Self {
        vec![num.0]
    }
}

#[tokio::test]
async fn broadcast_subscriber_end_to_end_test() {
    let topic1 = Topic::new("TOPIC1");
    let topic2 = Topic::new("TOPIC2");
    let mut bootstrap_swarm = create_swarm(None);
    let address = get_listen_address(&mut bootstrap_swarm).await;
    let bootstrap_peer_id = *bootstrap_swarm.local_peer_id();
    let bootstrap_peer_multiaddr = address.with_p2p(bootstrap_peer_id).unwrap();

    // Don't poll subscriber swarms before wrapping in network managers — polling would
    // discard the initial RequestDial events from bootstrapping before the network
    // manager is ready to route them.
    let mut swarm1 = create_swarm(Some(bootstrap_peer_multiaddr.clone()));
    let mut swarm2 = create_swarm(Some(bootstrap_peer_multiaddr));

    let all_peers: HashSet<PeerId> =
        HashSet::from([bootstrap_peer_id, *swarm1.local_peer_id(), *swarm2.local_peer_id()]);
    bootstrap_swarm.behaviour_mut().peer_whitelist.set_allowed_peers(all_peers.clone());
    swarm1.behaviour_mut().peer_whitelist.set_allowed_peers(all_peers.clone());
    swarm2.behaviour_mut().peer_whitelist.set_allowed_peers(all_peers);

    let bootstrap_network_manager = create_network_manager(bootstrap_swarm);
    let mut network_manager1 = create_network_manager(swarm1);
    let mut network_manager2 = create_network_manager(swarm2);

    let mut subscriber_channels1_1 =
        network_manager1.register_broadcast_topic::<Number>(topic1.clone(), BUFFER_SIZE).unwrap();
    let mut subscriber_channels1_2 =
        network_manager1.register_broadcast_topic::<Number>(topic2.clone(), BUFFER_SIZE).unwrap();

    let subscriber_channels2_1 =
        network_manager2.register_broadcast_topic::<Number>(topic1.clone(), BUFFER_SIZE).unwrap();
    let subscriber_channels2_2 =
        network_manager2.register_broadcast_topic::<Number>(topic2.clone(), BUFFER_SIZE).unwrap();

    tokio::select! {
        _ = network_manager1.run() => panic!("network manager ended"),
        _ = network_manager2.run() => panic!("network manager ended"),
        _ = bootstrap_network_manager.run() => panic!("network manager ended"),
        result = tokio::time::timeout(
            TIMEOUT, async move {
                // TODO(shahak): Remove this sleep once we fix the bug of broadcasting while there
                // are no peers.
                tokio::time::sleep(Duration::from_secs(1)).await;
                let number1 = Number(1);
                let number2 = Number(2);
                let mut broadcast_client2_1 =
                    subscriber_channels2_1.broadcasted_messages_receiver;
                let mut broadcast_client2_2 =
                    subscriber_channels2_2.broadcasted_messages_receiver;
                subscriber_channels1_1.broadcast_topic_client.broadcast_message(number1).await.unwrap();
                subscriber_channels1_2.broadcast_topic_client.broadcast_message(number2).await.unwrap();
                let (received_number1, _report_callback) =
                    broadcast_client2_1.next().await.unwrap();
                let (received_number2, _report_callback) =
                    broadcast_client2_2.next().await.unwrap();
                assert_eq!(received_number1.unwrap(), number1);
                assert_eq!(received_number2.unwrap(), number2);
                assert!(broadcast_client2_1.next().now_or_never().is_none());
                assert!(broadcast_client2_2.next().now_or_never().is_none());
            }
        ) => {
            result.unwrap()
        }
    }
}
