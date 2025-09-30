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
use crate::network_manager::{
    BroadcastTopicClientTrait,
    GenericNetworkManager,
    PropellerClientTrait,
};
use crate::peer_manager::PeerManagerConfig;
use crate::{sqmr, Bytes};

const TIMEOUT: Duration = Duration::from_secs(5);

async fn create_swarm(bootstrap_peer_multiaddr: Option<Vec<Multiaddr>>) -> Swarm<MixedBehaviour> {
    let mut swarm = Swarm::new_ephemeral_tokio(|keypair| {
        MixedBehaviour::new(
            sqmr::Config::default(),
            DiscoveryConfig::default(),
            PeerManagerConfig::default(),
            None,
            keypair.clone(),
            bootstrap_peer_multiaddr,
            ChainId::Mainnet,
            None,
        )
    });
    // Not using SwarmExt::listen because it panics if the swarm emits other events
    let expected_listener_id = swarm.listen_on(Protocol::Memory(0).into()).unwrap();
    let address = swarm
        .wait(|event| match event {
            SwarmEvent::NewListenAddr { listener_id, address }
                if expected_listener_id == listener_id =>
            {
                Some(address)
            }
            _ => None,
        })
        .await;
    swarm.add_external_address(address);

    swarm
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

async fn create_network_managers(
    num: usize,
) -> Vec<(PeerId, GenericNetworkManager<Swarm<MixedBehaviour>>)> {
    let mut bootstrap_addresses = vec![];
    let mut network_managers = vec![];
    for _ in 0..num {
        let swarm = create_swarm(if bootstrap_addresses.is_empty() {
            None
        } else {
            Some(bootstrap_addresses.clone())
        })
        .await;
        let local_peer_id = swarm.local_peer_id();
        let address = swarm.external_addresses().next().unwrap().clone();
        // Add peer ID to the multiaddr for bootstrap addresses
        let bootstrap_address = address.with_p2p(*local_peer_id).unwrap();
        bootstrap_addresses.push(bootstrap_address);
        network_managers.push((*local_peer_id, create_network_manager(swarm)));
    }
    network_managers
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
    let bootstrap_swarm = create_swarm(None).await;
    let bootstrap_peer_multiaddr = bootstrap_swarm.external_addresses().next().unwrap().clone();
    let bootstrap_peer_multiaddr =
        bootstrap_peer_multiaddr.with_p2p(*bootstrap_swarm.local_peer_id()).unwrap();
    let bootstrap_network_manager = create_network_manager(bootstrap_swarm);
    let mut network_manager1 =
        create_network_manager(create_swarm(Some(vec![bootstrap_peer_multiaddr.clone()])).await);
    let mut network_manager2 =
        create_network_manager(create_swarm(Some(vec![bootstrap_peer_multiaddr])).await);

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

#[tokio::test]
async fn propeller_message_forwarding_test() {
    // This test verifies that propeller messages are properly forwarded through channels
    // when received by the network manager, similar to how gossipsub messages work.

    let mut network_managers = create_network_managers(2).await;
    let (peer_1, mut nm_1) = network_managers.remove(0);
    let (peer_2, mut nm_2) = network_managers.remove(0);

    let peers = vec![(peer_1, 1000), (peer_2, 500)];

    let mut channels_1 =
        nm_1.register_propeller_channels::<Vec<u8>>(BUFFER_SIZE, peers.clone()).unwrap();
    let mut channels_2 = nm_2.register_propeller_channels::<Vec<u8>>(BUFFER_SIZE, peers).unwrap();

    let message = vec![123; 64]; // must be a multiple of 64
    let message_id = 1;

    tokio::select! {
        _ = nm_1.run() => panic!("network manager ended"),
        _ = nm_2.run() => panic!("network manager ended"),
        result = tokio::time::timeout(
            TIMEOUT, async move {
                tokio::time::sleep(Duration::from_secs(1)).await;

                println!("Sending message");
                channels_1.propeller_client.send_message(message.clone(), message_id).await.unwrap();

                println!("Receiving message");
                let (received_message_id, received_message) =
                    channels_2.propeller_messages_receiver.next().await.unwrap();

                assert_eq!(received_message.unwrap(), message);
                assert_eq!(received_message_id, message_id);

                assert!(channels_1.propeller_messages_receiver.next().now_or_never().is_none());
                assert!(channels_2.propeller_messages_receiver.next().now_or_never().is_none());
            }
        ) => {
            result.unwrap()
        }
    }
}
