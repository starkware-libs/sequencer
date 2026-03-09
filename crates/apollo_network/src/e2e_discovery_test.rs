use std::collections::{HashMap, HashSet};
use std::time::Duration;

use futures::StreamExt;
use libp2p::core::multiaddr::Protocol;
use libp2p::swarm::SwarmEvent;
use libp2p::{Multiaddr, PeerId, Swarm};
use libp2p_swarm_test::SwarmExt;
use starknet_api::core::ChainId;

use crate::discovery::DiscoveryConfig;
use crate::mixed_behaviour::{self, MixedBehaviour};
use crate::peer_manager::PeerManagerConfig;
use crate::prune_dead_connections::{DEFAULT_PING_INTERVAL, DEFAULT_PING_TIMEOUT};
use crate::sqmr;

async fn create_swarm(bootstrap_peer_multiaddr: Option<Multiaddr>) -> Swarm<MixedBehaviour> {
    let mut swarm = Swarm::new_ephemeral_tokio(|keypair| {
        MixedBehaviour::new(
            sqmr::Config::default(),
            DiscoveryConfig { random_peer_request_enabled: false, ..Default::default() },
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

/// Routes inter-behaviour events the same way the network manager does.
fn route_internal_event(swarm: &mut Swarm<MixedBehaviour>, event: mixed_behaviour::Event) {
    if let mixed_behaviour::Event::ToOtherBehaviourEvent(internal_event) = event {
        swarm.behaviour_mut().route_to_other_behaviour_event(internal_event);
    }
}

#[tokio::test]
async fn all_peers_discover_each_other_when_given_peer_ids() {
    const NUM_PEERS: usize = 100;

    // Create bootstrap node.
    let bootstrap_swarm = create_swarm(None).await;
    let bootstrap_peer_id = *bootstrap_swarm.local_peer_id();
    let bootstrap_multiaddr = bootstrap_swarm
        .external_addresses()
        .next()
        .unwrap()
        .clone()
        .with_p2p(bootstrap_peer_id)
        .unwrap();

    // Create peer swarms, all bootstrapping through the same node.
    let mut swarms = Vec::with_capacity(NUM_PEERS);
    for _ in 0..NUM_PEERS {
        swarms.push(create_swarm(Some(bootstrap_multiaddr.clone())).await);
    }

    // Oracle: collect all peer IDs (excluding bootstrap).
    let all_peer_ids: HashSet<PeerId> = swarms.iter().map(|s| *s.local_peer_id()).collect();

    // Tell each swarm's discovery behaviour about all peers.
    for swarm in &mut swarms {
        if let Some(discovery) = swarm.behaviour_mut().discovery.as_mut() {
            discovery.set_peers_to_request(all_peer_ids.clone());
        }
    }

    let (connection_sender, mut connection_receiver) =
        tokio::sync::mpsc::unbounded_channel::<(PeerId, PeerId)>();

    // Spawn the bootstrap swarm — routes internal events so Kademlia works.
    tokio::spawn(async move {
        let mut bootstrap_swarm = bootstrap_swarm;
        while let Some(event) = bootstrap_swarm.next().await {
            if let SwarmEvent::Behaviour(behaviour_event) = event {
                route_internal_event(&mut bootstrap_swarm, behaviour_event);
            }
        }
    });

    // Spawn each peer swarm — routes internal events and reports connections.
    for mut swarm in swarms {
        let sender = connection_sender.clone();
        let peer_ids = all_peer_ids.clone();
        tokio::spawn(async move {
            let local_peer_id = *swarm.local_peer_id();
            while let Some(event) = swarm.next().await {
                match event {
                    SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                        if peer_id != bootstrap_peer_id && peer_ids.contains(&peer_id) {
                            let _ = sender.send((local_peer_id, peer_id));
                        }
                    }
                    SwarmEvent::Behaviour(behaviour_event) => {
                        route_internal_event(&mut swarm, behaviour_event);
                    }
                    _ => {}
                }
            }
        });
    }
    drop(connection_sender);

    // Collect connections until full mesh or timeout.
    let mut connections: HashMap<PeerId, HashSet<PeerId>> =
        all_peer_ids.iter().map(|id| (*id, HashSet::new())).collect();
    let total_expected = NUM_PEERS * (NUM_PEERS - 1);

    let result = tokio::time::timeout(Duration::from_secs(120), async {
        let start = tokio::time::Instant::now();
        let mut last_print = start;
        while let Some((from, to)) = connection_receiver.recv().await {
            connections.get_mut(&from).unwrap().insert(to);
            let total_connections: usize = connections.values().map(|s| s.len()).sum();
            if total_connections >= total_expected {
                return;
            }
            let now = tokio::time::Instant::now();
            if now - last_print > Duration::from_secs(5) {
                eprintln!(
                    "[{:.1}s] {total_connections}/{total_expected} connections",
                    (now - start).as_secs_f64()
                );
                last_print = now;
            }
        }
    })
    .await;

    if result.is_err() {
        let total_connections: usize = connections.values().map(|s| s.len()).sum();
        panic!(
            "Timed out waiting for full connectivity: {total_connections}/{total_expected} \
             connections established"
        );
    }
}
