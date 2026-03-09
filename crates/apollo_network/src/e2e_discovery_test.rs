use std::collections::{HashMap, HashSet};
use std::time::Duration;

use futures::StreamExt;
use libp2p::core::multiaddr::Protocol;
use libp2p::swarm::SwarmEvent;
use libp2p::{Multiaddr, PeerId, Swarm};
use libp2p_swarm_test::SwarmExt;
use starknet_api::core::ChainId;

const TEST_TIMEOUT: Duration = Duration::from_secs(30);
const DISCOVERY_HEARTBEAT_INTERVAL: Duration = Duration::from_millis(1000);
const NUM_PEERS: usize = 100;

use crate::discovery::DiscoveryConfig;
use crate::mixed_behaviour::{self, MixedBehaviour};
use crate::peer_manager::PeerManagerConfig;
use crate::prune_dead_connections::{DEFAULT_PING_INTERVAL, DEFAULT_PING_TIMEOUT};
use crate::sqmr;

async fn create_swarm(bootstrap_peer_multiaddr: Option<Multiaddr>) -> Swarm<MixedBehaviour> {
    let mut swarm = Swarm::new_ephemeral_tokio(|keypair| {
        MixedBehaviour::new(
            sqmr::Config::default(),
            DiscoveryConfig {
                random_peer_request_enabled: false,
                heartbeat_interval: DISCOVERY_HEARTBEAT_INTERVAL,
                ..Default::default()
            },
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

fn print_progress(
    start: tokio::time::Instant,
    connections: &HashMap<PeerId, HashSet<PeerId>>,
    bootstrap_connections: &HashSet<PeerId>,
    ordered_peer_ids: &[PeerId],
    total_expected: usize,
) {
    let total: usize = connections.values().map(|s| s.len()).sum();
    let boot_count = bootstrap_connections.len();
    let per_peer: String = ordered_peer_ids
        .iter()
        .map(|id| connections[id].len().to_string())
        .collect::<Vec<_>>()
        .join(" ");
    let elapsed = start.elapsed().as_secs_f64();
    println!(
        "{elapsed:.1}s total_connections: {total}/{total_expected} bootstrap: \
         {boot_count}/{NUM_PEERS} per_peer: {per_peer}"
    );
}

/// Routes inter-behaviour events the same way the network manager does.
fn route_internal_event(swarm: &mut Swarm<MixedBehaviour>, event: mixed_behaviour::Event) {
    if let mixed_behaviour::Event::ToOtherBehaviourEvent(internal_event) = event {
        swarm.behaviour_mut().route_to_other_behaviour_event(internal_event);
    }
}

/// Validates that 100 peers can discover each other through a single bootstrap node.
/// This is a long-running test (~30s in release mode) and is ignored by default.
/// Run intentionally with: `cargo test -p apollo_network --release e2e_discovery -- --ignored
/// --no-capture`
#[ignore]
#[tokio::test]
async fn all_peers_discover_each_other_when_given_peer_ids() {
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

    let all_peer_ids: HashSet<PeerId> = swarms.iter().map(|s| *s.local_peer_id()).collect();
    // Stable ordering for display columns.
    let ordered_peer_ids: Vec<PeerId> = {
        let mut ids: Vec<PeerId> = all_peer_ids.iter().copied().collect();
        ids.sort_by_key(|id| id.to_string());
        ids
    };

    // Tell each swarm's discovery behaviour about all peers.
    for swarm in &mut swarms {
        swarm
            .behaviour_mut()
            .discovery
            .as_mut()
            .unwrap()
            .set_peers_to_request(all_peer_ids.clone());
    }

    // Two channels: one for peer-to-peer connections, one for bootstrap connections.
    let (connection_sender, mut connection_receiver) =
        tokio::sync::mpsc::unbounded_channel::<(PeerId, PeerId)>();
    let (bootstrap_sender, mut bootstrap_receiver) =
        tokio::sync::mpsc::unbounded_channel::<PeerId>();

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
        let boot_sender = bootstrap_sender.clone();
        let peer_ids = all_peer_ids.clone();
        tokio::spawn(async move {
            let local_peer_id = *swarm.local_peer_id();
            while let Some(event) = swarm.next().await {
                match event {
                    SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                        if peer_id == bootstrap_peer_id {
                            let _ = boot_sender.send(local_peer_id);
                        } else if peer_ids.contains(&peer_id) {
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
    drop(bootstrap_sender);

    // Collect connections until full mesh or timeout, printing progress every second.
    let mut connections: HashMap<PeerId, HashSet<PeerId>> =
        all_peer_ids.iter().map(|id| (*id, HashSet::new())).collect();
    let mut bootstrap_connections: HashSet<PeerId> = HashSet::new();
    let total_expected = NUM_PEERS * (NUM_PEERS - 1);
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let start = tokio::time::Instant::now();

    let result = tokio::time::timeout(TEST_TIMEOUT, async {
        loop {
            tokio::select! {
                msg = connection_receiver.recv() => {
                    match msg {
                        Some((from, to)) => {
                            connections.get_mut(&from).unwrap().insert(to);
                            let total: usize = connections.values().map(|s| s.len()).sum();
                            if total >= total_expected {
                                return;
                            }
                        }
                        None => return,
                    }
                }
                msg = bootstrap_receiver.recv() => {
                    if let Some(peer_id) = msg {
                        bootstrap_connections.insert(peer_id);
                    }
                }
                _ = tick.tick() => {
                    print_progress(start, &connections, &bootstrap_connections, &ordered_peer_ids, total_expected);
                }
            }
        }
    })
    .await;

    print_progress(start, &connections, &bootstrap_connections, &ordered_peer_ids, total_expected);
    let total_connections: usize = connections.values().map(|s| s.len()).sum();

    if result.is_err() {
        let mut counts: Vec<_> = connections.iter().map(|(id, s)| (s.len(), id)).collect();
        counts.sort();
        eprintln!("\nPer-peer connection counts (sorted):");
        for (count, peer_id) in &counts {
            eprintln!("  {count:>3}/{}  {peer_id}", NUM_PEERS - 1);
        }
        panic!(
            "Timed out waiting for full connectivity: {total_connections}/{total_expected} \
             connections established"
        );
    }
}
