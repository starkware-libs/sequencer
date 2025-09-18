use std::time::Duration;

use apollo_metrics::metrics::MetricGauge;
use futures::{FutureExt, StreamExt};
use libp2p::core::multiaddr::Protocol;
use libp2p::swarm::SwarmEvent;
use libp2p::{Multiaddr, Swarm};
use libp2p_swarm_test::SwarmExt;
use metrics_exporter_prometheus::PrometheusBuilder;
use starknet_api::core::ChainId;

use crate::discovery::DiscoveryConfig;
use crate::gossipsub_impl::Topic;
use crate::mixed_behaviour::MixedBehaviour;
use crate::network_manager::metrics::NetworkMetrics;
use crate::network_manager::{BroadcastTopicClientTrait, GenericNetworkManager, NetworkManager};
use crate::peer_manager::PeerManagerConfig;
use crate::{sqmr, Bytes};

const NUM_CONNECTED_PEERS: MetricGauge = MetricGauge::new(
    apollo_metrics::metrics::MetricScope::MempoolP2p,
    "num_connected_peers",
    "Number of connected peers",
);
const NUM_BLACKLISTED_PEERS: MetricGauge = MetricGauge::new(
    apollo_metrics::metrics::MetricScope::MempoolP2p,
    "num_blacklisted_peers",
    "Number of blacklisted peers",
);

const TIMEOUT: Duration = Duration::from_secs(5);

async fn create_swarm(bootstrap_peer_multiaddr: Option<Multiaddr>) -> Swarm<MixedBehaviour> {
    let mut swarm = Swarm::new_ephemeral_tokio(|keypair| {
        MixedBehaviour::new(
            sqmr::Config::default(),
            DiscoveryConfig::default(),
            PeerManagerConfig::default(),
            None,
            keypair.clone(),
            bootstrap_peer_multiaddr.map(|multiaddr| vec![multiaddr]),
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
    let metrics = NetworkMetrics {
        num_connected_peers: NUM_CONNECTED_PEERS,
        num_blacklisted_peers: NUM_BLACKLISTED_PEERS,
        broadcast_metrics_by_topic: None,
        sqmr_metrics: None,
        event_metrics: None,
    };
    GenericNetworkManager::generic_new(
        swarm,
        None,
        Some(metrics),
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

pub async fn create_peer_network(num_peers: usize) -> Vec<NetworkManager> {
    assert!(num_peers > 0, "Number of peers must be greater than 0");

    let get_address = |swarm: &Swarm<MixedBehaviour>| {
        let address = swarm.external_addresses().next().unwrap().clone();
        address.with_p2p(*swarm.local_peer_id()).unwrap()
    };
    let mut network_managers = Vec::new();

    let bootstrap_swarm = create_swarm(None).await;
    let bootstrap_peer_multiaddr = get_address(&bootstrap_swarm);
    network_managers.push(create_network_manager(bootstrap_swarm));

    for _ in 0..num_peers {
        let swarm = create_swarm(Some(bootstrap_peer_multiaddr.clone())).await;
        network_managers.push(create_network_manager(swarm));
    }

    network_managers
}

#[tokio::test]
async fn broadcast_subscriber_end_to_end_test() {
    let topic1 = Topic::new("TOPIC1");
    let topic2 = Topic::new("TOPIC2");

    let mut network_managers = create_peer_network(2).await;
    let bootstrap_network_manager = network_managers.remove(0);
    let mut network_manager1 = network_managers.remove(0);
    let mut network_manager2 = network_managers.remove(0);

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

#[cfg(any(feature = "testing", test))]
#[tokio::test]
async fn test_report_peer_metric() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);

    let mut network_managers = create_peer_network(2).await;
    let bootstrap_network_manager = network_managers.remove(0);
    let mut network_manager1 = network_managers.remove(0);
    let mut network_manager2 = network_managers.remove(0);

    let topic = Topic::new("TOPIC");

    let mut subscriber_channels1 =
        network_manager1.register_broadcast_topic::<Number>(topic.clone(), BUFFER_SIZE).unwrap();
    let mut subscriber_channels2 =
        network_manager2.register_broadcast_topic::<Number>(topic.clone(), BUFFER_SIZE).unwrap();

    tokio::select! {
        _ = network_manager1.run() => panic!("network manager ended"),
        _ = network_manager2.run() => panic!("network manager ended"),
        _ = bootstrap_network_manager.run() => panic!("network manager ended"),
        result = tokio::time::timeout(
            TIMEOUT, async  {
                tokio::time::sleep(Duration::from_secs(1)).await;

                let metrics = recorder.handle().render();
                NUM_CONNECTED_PEERS.assert_eq(&metrics, 6);
                // TODO(AndrewL): adding the next line causes the test to flakey.
                // (sometimes peers are blacklisted for no reason?) This because
                // dial failures result in reported peers.
                // NUM_BLACKLISTED_PEERS.assert_eq(&metrics, 0);

                // Broadcast a message from network_manager1 to network_manager2
                subscriber_channels1.broadcast_topic_client.broadcast_message(Number(1)).await.unwrap();

                // Receive the message from network_manager1
                let (received_number1, report_callback) =
                subscriber_channels2.broadcasted_messages_receiver.next().await.unwrap();
                assert_eq!(received_number1.unwrap(), Number(1));

                // Report the peer to network_manager2
                subscriber_channels2.broadcast_topic_client.report_peer(report_callback).await.unwrap();

                // Allow time for the peer report to be processed
                tokio::time::sleep(Duration::from_secs(1)).await;

                // Check the metrics
                let metrics = recorder.handle().render();

                // With 3 network managers (1 bootstrap + 2 peers), each connecting to the others,
                // we get 6 total connections (3 managers × 2 connections each)
                NUM_CONNECTED_PEERS.assert_eq(&metrics, 6u64);
                assert!(NUM_BLACKLISTED_PEERS.parse_numeric_metric::<u64>(&metrics).unwrap() > 0);
            },
        ) => {
            result.unwrap()
        }
    }
}
