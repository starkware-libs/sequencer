use std::time::{Duration, Instant};

use apollo_infra::trace_util::configure_tracing;
use futures::{FutureExt, StreamExt};
use libp2p::core::multiaddr::Protocol;
use libp2p::swarm::SwarmEvent;
use libp2p::{Multiaddr, Swarm};
use libp2p_swarm_test::SwarmExt;
use starknet_api::core::ChainId;
use tracing::info;

use crate::discovery::DiscoveryConfig;
use crate::gossipsub_impl::Topic;
use crate::mixed_behaviour::MixedBehaviour;
use crate::network_manager::{BroadcastTopicClientTrait, GenericNetworkManager};
use crate::peer_manager::PeerManagerConfig;
use crate::{sqmr, Bytes};

const TIMEOUT: Duration = Duration::from_secs(5);

async fn create_swarm(bootstrap_peer_multiaddr: Option<Multiaddr>) -> Swarm<MixedBehaviour> {
    let mut swarm = Swarm::new_ephemeral(|keypair| {
        MixedBehaviour::new(
            keypair.clone(),
            bootstrap_peer_multiaddr.map(|multiaddr| vec![multiaddr]),
            sqmr::Config { session_timeout: Duration::from_secs(100) },
            ChainId::Mainnet,
            None,
            DiscoveryConfig::default(),
            PeerManagerConfig::default(),
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
        create_network_manager(create_swarm(Some(bootstrap_peer_multiaddr.clone())).await);
    let mut network_manager2 =
        create_network_manager(create_swarm(Some(bootstrap_peer_multiaddr)).await);

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

async fn make_peers(n: usize) -> Vec<GenericNetworkManager<Swarm<MixedBehaviour>>> {
    let mut bootstrap_addresses = vec![];
    let mut peers = vec![];
    for _ in 0..n {
        let swarm = create_swarm(bootstrap_addresses.first().cloned()).await;
        let multiaddr = swarm.external_addresses().next().unwrap().clone();
        let multiaddr = multiaddr.with_p2p(*swarm.local_peer_id()).unwrap();
        let peer = create_network_manager(swarm);

        bootstrap_addresses.push(multiaddr);
        peers.push(peer);
    }

    peers
}

type BigMessage = Vec<u8>;

#[tokio::test]
async fn broadcast_subscriber_end_to_end_test_throughput_test() {
    configure_tracing().await;

    const MESSAGE_SIZE: usize = 1 << 20;
    let message1: BigMessage = vec![1; MESSAGE_SIZE];

    let topic1 = Topic::new("/TOPIC1");
    let peers = make_peers(2).await;

    // Move peers out of vector to avoid ownership issues
    let mut peers_iter = peers.into_iter();
    let mut peer1 = peers_iter.next().unwrap();
    let mut peer2 = peers_iter.next().unwrap();

    let mut p1c = peer1
        .register_sqmr_protocol_client::<BigMessage, BigMessage>(topic1.to_string(), BUFFER_SIZE);
    let mut p2s = peer2
        .register_sqmr_protocol_server::<BigMessage, BigMessage>(topic1.to_string(), BUFFER_SIZE);

    tokio::select! {
        _ = peer1.run() => panic!("network manager ended"),
        _ = peer2.run() => panic!("network manager ended"),
        result = tokio::time::timeout(
            Duration::from_secs(10), async move { // Reduced timeout for debugging
                tokio::time::sleep(Duration::from_secs(1)).await;
                let start_time = Instant::now();

                info!("Step 1: Sending query");
                let mut handle1 = p1c.send_new_query(message1.clone()).await.unwrap(); // Send actual message instead of empty vec

                info!("Step 2: Waiting for query on server");
                let mut server_query_manager = p2s.next().await.unwrap();
                info!("Step 2: Received query: {}", server_query_manager.query().as_ref().unwrap().len());

                info!("Step 3: Sending response");
                server_query_manager.send_response(vec![]).await.unwrap();

                info!("Step 3.5: Dropping server query manager to close response stream");
                // Drop the ServerQueryManager, which should trigger the Drop trait and close the response stream
                drop(server_query_manager);
                info!("Server query manager dropped");

                info!("Step 4: Waiting for response on client");
                let response = handle1.next().await.unwrap().unwrap();
                info!("Step 4: Received response: {}", response.len());

                info!("Test completed successfully!");
                info!("Elapsed = {}", start_time.elapsed().as_secs_f32());
                info!("Throughput: {:.2} MB/s", (MESSAGE_SIZE as f64) / (1024.0 * 1024.0) / start_time.elapsed().as_secs_f64());

                assert!(start_time.elapsed() < Duration::from_secs(1));
            }
        ) => {
            result.unwrap()
        }
    }
}

#[tokio::test]
async fn broadcast_topic_throughput_test() {
    configure_tracing().await;

    const MESSAGE_SIZE: usize = 1 << 20; // 1MB message
    let large_message: BigMessage = vec![42; MESSAGE_SIZE];

    let topic1 = Topic::new("/BROADCAST_THROUGHPUT_TOPIC");
    let peers = make_peers(2).await;

    // Move peers out of vector to avoid ownership issues
    let mut peers_iter = peers.into_iter();
    let mut peer1 = peers_iter.next().unwrap();
    let mut peer2 = peers_iter.next().unwrap();

    let mut subscriber_channels1 =
        peer1.register_broadcast_topic::<BigMessage>(topic1.clone(), BUFFER_SIZE).unwrap();
    let subscriber_channels2 =
        peer2.register_broadcast_topic::<BigMessage>(topic1.clone(), BUFFER_SIZE).unwrap();

    tokio::select! {
        _ = peer1.run() => panic!("network manager ended"),
        _ = peer2.run() => panic!("network manager ended"),
        result = tokio::time::timeout(
            Duration::from_secs(10), async move {
                // Wait for peers to connect
                tokio::time::sleep(Duration::from_secs(1)).await;
                let start_time = Instant::now();

                info!("Step 1: Broadcasting large message ({} bytes)", MESSAGE_SIZE);
                subscriber_channels1.broadcast_topic_client
                    .broadcast_message(large_message.clone())
                    .await
                    .unwrap();

                info!("Step 2: Waiting for broadcast message on peer2");
                let mut broadcast_receiver = subscriber_channels2.broadcasted_messages_receiver;
                let (received_message, _report_callback) = broadcast_receiver.next().await.unwrap();
                let received_message = received_message.unwrap();

                info!("Step 3: Received message with {} bytes", received_message.len());
                assert_eq!(received_message, large_message);

                let elapsed = start_time.elapsed();
                info!("Broadcast throughput test completed successfully!");
                info!("Elapsed time: {:.3}s", elapsed.as_secs_f32());
                info!("Throughput: {:.2} MB/s", (MESSAGE_SIZE as f64) / (1024.0 * 1024.0) / elapsed.as_secs_f64());
                assert!(elapsed < Duration::from_secs(1));

                // Verify no additional messages
                assert!(broadcast_receiver.next().now_or_never().is_none());
            }
        ) => {
            result.unwrap()
        }
    }
}
