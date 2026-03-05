use std::collections::HashMap;
use std::time::Duration;

use apollo_propeller::types::{CommitteeId, Event};
use apollo_propeller::{Behaviour, Config};
use futures::StreamExt;
use libp2p::swarm::SwarmEvent;
use libp2p::{PeerId, Swarm};
use libp2p_swarm_test::SwarmExt;
use rstest::rstest;
use tokio::sync::mpsc;
use tracing_test::traced_test;

const TIMEOUT: Duration = Duration::from_secs(5);

type NodeIndex = usize;

// TODO(AndrewL): unite test setup code with apollo_network tests.
struct TestSetup {
    swarms: Vec<Swarm<Behaviour>>,
}

impl TestSetup {
    async fn new(n: usize) -> Self {
        let mut swarms = Vec::new();
        for _ in 0..n {
            let mut swarm = Swarm::new_ephemeral_tokio(|keypair| {
                Behaviour::new(keypair.clone(), Config::default())
            });
            swarm.listen().with_memory_addr_external().await;
            swarms.push(swarm);
        }
        for i in 0..n {
            for j in (i + 1)..n {
                // split_at_mut lets us borrow two elements mutably at the same time.
                let (a, b) = swarms.split_at_mut(j);
                a[i].connect(&mut b[0]).await;
            }
        }
        Self { swarms }
    }

    fn num_nodes(&self) -> usize {
        self.swarms.len()
    }

    async fn register_committee(&mut self, committee_id: CommitteeId, node_indices: &[NodeIndex]) {
        let peers: Vec<(PeerId, u64)> =
            node_indices.iter().map(|&i| (*self.swarms[i].local_peer_id(), 1)).collect();
        for &i in node_indices {
            self.swarms[i]
                .behaviour_mut()
                .register_committee_peers(committee_id, peers.clone())
                .await
                .unwrap()
                .expect("Failed to register committee");
        }
    }

    async fn broadcast(
        &mut self,
        node_idx: NodeIndex,
        committee_id: CommitteeId,
        message: Vec<u8>,
    ) {
        self.swarms[node_idx]
            .behaviour_mut()
            .broadcast(committee_id, message)
            .await
            .unwrap()
            .expect("Broadcast should succeed");
    }

    /// Drives all swarms, collecting delivered messages, then asserts expectations.
    /// `expected`: slice of (node_index, expected_message_payloads).
    /// Nodes not listed are expected to receive nothing; stray messages cause failure.
    async fn run_with_expectations(self, expected: &[(NodeIndex, Vec<Vec<u8>>)]) {
        let peer_ids: Vec<PeerId> = self.swarms.iter().map(|s| *s.local_peer_id()).collect();
        let expected_map: HashMap<PeerId, &Vec<Vec<u8>>> =
            expected.iter().map(|(idx, msgs)| (peer_ids[*idx], msgs)).collect();

        let mut receivers: HashMap<PeerId, mpsc::UnboundedReceiver<(PeerId, Vec<u8>)>> =
            HashMap::new();
        for mut swarm in self.swarms {
            let peer_id = *swarm.local_peer_id();
            let (message_tx, message_rx) = mpsc::unbounded_channel();
            tokio::spawn(async move {
                loop {
                    if let SwarmEvent::Behaviour(Event::MessageReceived {
                        publisher,
                        message,
                        ..
                    }) = swarm.select_next_some().await
                    {
                        let _ = message_tx.send((publisher, message));
                    }
                }
            });
            receivers.insert(peer_id, message_rx);
        }

        for (peer_id, expected_msgs) in &expected_map {
            let rx = receivers.get_mut(peer_id).expect("Missing receiver");
            let mut received = Vec::with_capacity(expected_msgs.len());
            let result = tokio::time::timeout(TIMEOUT, async {
                while received.len() < expected_msgs.len() {
                    received.push(rx.recv().await.expect("Swarm driver dropped unexpectedly"));
                }
            })
            .await;
            assert!(
                result.is_ok(),
                "Timed out for {peer_id}: received {}/{} messages",
                received.len(),
                expected_msgs.len()
            );
            let mut got: Vec<&[u8]> = received.iter().map(|(_, m)| m.as_slice()).collect();
            let mut want: Vec<&[u8]> = expected_msgs.iter().map(|m| m.as_slice()).collect();
            // Propeller does not guarantee delivery order.
            got.sort();
            want.sort();
            assert_eq!(got, want, "Peer {peer_id} received wrong messages");
        }

        let mut merged = futures::stream::select_all(
            receivers.into_values().map(tokio_stream::wrappers::UnboundedReceiverStream::new),
        );
        let stray = tokio::time::timeout(Duration::from_millis(200), merged.next()).await;
        assert!(stray.is_err(), "A node received an unexpected extra message");
    }
}

#[traced_test]
#[rstest]
// TODO(AndrewL): make the 1-node case pass.
#[case(2, 17, 1)] // Minimal pair, small payload, single publisher (1 data shard, 0 coding shards).
#[case(3, 65536, 1)] // Odd count, large payload, single publisher (1 data shard, 1 coding shard).
#[case(10, 65536, 2)]
// Larger network with more shards (3 data, 6 coding) and multiple publishers.
// `current_thread` is required so that `tracing-test` (which installs a thread-local
// subscriber) captures logs from `tokio::spawn`ed tasks on the same thread.
#[tokio::test(flavor = "current_thread")]
async fn e2e_broadcast_single_message(
    #[case] num_nodes: usize,
    #[case] message_size: usize,
    #[case] num_publishers: usize,
) {
    let mut setup = TestSetup::new(num_nodes).await;
    let committee_id = CommitteeId(0);
    let all: Vec<NodeIndex> = (0..num_nodes).collect();
    setup.register_committee(committee_id, &all).await;

    let mut messages: Vec<Vec<u8>> = Vec::new();
    for pub_idx in 0..num_publishers {
        let msg: Vec<u8> =
            (0..message_size).map(|i| u8::try_from((i + pub_idx) % 256).unwrap()).collect();
        setup.broadcast(pub_idx, committee_id, msg.clone()).await;
        messages.push(msg);
    }

    let expected: Vec<(NodeIndex, Vec<Vec<u8>>)> = (0..setup.num_nodes())
        .filter_map(|idx| {
            let msgs: Vec<Vec<u8>> = messages
                .iter()
                .enumerate()
                .filter(|&(pub_idx, _)| pub_idx != idx)
                .map(|(_, m)| m.clone())
                .collect();
            if msgs.is_empty() { None } else { Some((idx, msgs)) }
        })
        .collect();

    setup.run_with_expectations(&expected).await;
    assert!(!logs_contain("WARN"));
    assert!(!logs_contain("ERROR"));
}

/// 6 nodes (A..F), two overlapping committees:
///   Committee 0 = {A, B, C, D}
///   Committee 1 = {C, D, E, F}
/// C broadcasts a different message on each committee.
/// Verify each committee's members (minus C) receive the correct message,
/// and no node receives a message from a committee it is not part of.
#[traced_test]
#[tokio::test(flavor = "current_thread")]
async fn e2e_committee_isolation() {
    let mut setup = TestSetup::new(6).await;
    setup.register_committee(CommitteeId(0), &[0, 1, 2, 3]).await;
    setup.register_committee(CommitteeId(1), &[2, 3, 4, 5]).await;

    let msg_ch0 = b"message-for-committee-0".to_vec();
    let msg_ch1 = b"message-for-committee-1".to_vec();

    setup.broadcast(2, CommitteeId(0), msg_ch0.clone()).await;
    setup.broadcast(2, CommitteeId(1), msg_ch1.clone()).await;

    let expected = vec![
        (0, vec![msg_ch0.clone()]),
        (1, vec![msg_ch0.clone()]),
        (3, vec![msg_ch0, msg_ch1.clone()]),
        (4, vec![msg_ch1.clone()]),
        (5, vec![msg_ch1]),
    ];

    setup.run_with_expectations(&expected).await;
    assert!(!logs_contain("WARN"));
    assert!(!logs_contain("ERROR"));
}
