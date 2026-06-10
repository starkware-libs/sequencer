use std::collections::BTreeMap;

use libp2p::PeerId;
use rstest::rstest;

use crate::behaviour_test_utils::TestEnvironment;
use crate::config::Config;
use crate::PropellerUnit;

#[rstest]
// TODO(AndrewL): make case(1) work. A single-node committee currently has no recipients to drive
// the broadcast/reception flow.
#[case(2)]
#[case(3)]
#[case(4)]
#[case(5)]
#[case(6)]
#[case(7)]
#[case(8)]
#[case(9)]
#[case(10)]
#[tokio::test]
async fn test_broadcast_and_receive(#[case] num_nodes: usize) {
    let config = Config::default();
    let mut env = TestEnvironment::new(num_nodes, config);
    env.simulate_connect_all();
    let committee_id = env.register_committee().await;
    let publisher_id = env.peer_ids()[0];
    let message = b"Hello, Propeller!".to_vec();

    env.node_mut(publisher_id)
        .behaviour
        .broadcast(committee_id, message.clone())
        .await
        .unwrap()
        .unwrap();

    // Step 1: Publisher sends initial shards to designated peers (one per recipient).
    let mut initial_shards: BTreeMap<PeerId, PropellerUnit> = BTreeMap::new();
    for _ in 0..(num_nodes - 1) {
        let (recipient, unit) = env.node_mut(publisher_id).expect_send_unit().await;
        assert_eq!(unit.committee_id(), committee_id);
        assert_eq!(unit.publisher(), publisher_id);
        initial_shards.insert(recipient, unit);
    }
    // Each recipient must get a distinct shard; initial_shards is a BTreeMap, so a duplicate
    // recipient would be silently overwritten and shrink the set below num_nodes - 1.
    assert_eq!(initial_shards.len(), num_nodes - 1);
    env.node_mut(publisher_id).expect_no_events().await;

    let recipient_ids: Vec<PeerId> = initial_shards.keys().copied().collect();

    // Step 2: Each recipient simulates receiving its shard from the publisher; collect the gossip
    // it broadcasts to the other recipients. Each recipient gossips to (num_nodes - 2) peers.
    let mut gossip_to_deliver: Vec<(PeerId, PeerId, PropellerUnit)> = Vec::new();
    for &recipient_id in &recipient_ids {
        let unit = initial_shards.get(&recipient_id).unwrap().clone();
        env.node_mut(recipient_id).simulate_receive_unit(publisher_id, unit);

        // Verify the recipient forwarded its shard to every other recipient (and only those).
        let mut expected_targets: Vec<PeerId> =
            recipient_ids.iter().copied().filter(|&peer| peer != recipient_id).collect();
        for _ in 0..expected_targets.len() {
            let (target, gossip_unit) = env.node_mut(recipient_id).expect_send_unit().await;
            assert_eq!(gossip_unit.committee_id(), committee_id);
            assert_eq!(gossip_unit.publisher(), publisher_id);
            assert!(expected_targets.contains(&target), "Gossip to unexpected peer: {:?}", target);
            expected_targets.retain(|&peer| peer != target);
            gossip_to_deliver.push((recipient_id, target, gossip_unit));
        }
    }

    // Step 3: Simulate each target receiving every gossipped shard. The sender must match the
    // designated broadcaster of the shard (each recipient broadcasts its own shard).
    for (sender_id, target_id, unit) in gossip_to_deliver {
        env.node_mut(target_id).simulate_receive_unit(sender_id, unit);
    }

    // Step 4: Every recipient should reconstruct the message and emit MessageReceived with
    // the original payload.
    for &recipient_id in &recipient_ids {
        let (recv_publisher, recv_message) =
            env.node_mut(recipient_id).expect_message_received().await;
        assert_eq!(recv_publisher, publisher_id);
        assert_eq!(recv_message, message);
    }
}

/// Minimal test that reproduces the shard counting bug in PostReconstruction phase.
///
/// This test creates a 5-node network where:
/// 1. Node receives its first shard and starts reconstruction immediately (since should_build(1) =
///    true)
/// 2. After reconstruction, it needs more shards to reach access threshold (should_receive(2) =
///    true for 5+ nodes)
/// 3. Additional shards arrive via gossip
/// 4. Node should emit MessageReceived event (this failed before the fix)
#[tokio::test]
async fn test_post_reconstruction_shard_counting() {
    // Setup: 5 nodes (1 publisher + 4 recipients)
    let config = Config::default();
    let mut env = TestEnvironment::new(5, config);
    env.simulate_connect_all();
    let committee_id = env.register_committee().await;

    let peer_ids = env.peer_ids();
    let publisher_id = peer_ids[0];
    let recipient_id = peer_ids[1]; // The node we're testing

    // Publisher broadcasts a message
    let message = vec![42u8; 1024];
    env.node_mut(publisher_id)
        .behaviour
        .broadcast(committee_id, message.clone())
        .await
        .unwrap()
        .unwrap();

    // Collect initial broadcast from publisher (4 shards to 4 recipients)
    let mut initial_shards = BTreeMap::new();
    for _ in 0..4 {
        let (peer, unit) = env.node_mut(publisher_id).expect_send_unit().await;
        initial_shards.insert(peer, unit);
    }

    // Recipient receives its assigned shard (index 0) from publisher
    // This triggers immediate reconstruction since should_build(1) = true
    let recipient_shard = initial_shards.get(&recipient_id).unwrap().clone();
    env.node_mut(recipient_id).simulate_receive_unit(publisher_id, recipient_shard.clone());

    // Recipient gossips its shard to other recipients
    for _ in 0..3 {
        let (_peer, _unit) = env.node_mut(recipient_id).expect_send_unit().await;
    }

    // Now recipient receives one additional shard from another recipient.
    // This is the critical moment: before the fix, the additional_shards counter was not
    // incremented.
    // The sender must be the designated broadcaster for the shard (validate_origin checks this),
    // so we pick a shard that was sent to a peer other than recipient_id and deliver it from
    // that peer.
    let (&another_recipient_id, another_shard) =
        initial_shards.iter().find(|(&peer, _)| peer != recipient_id).unwrap();
    let another_shard = another_shard.clone();
    env.node_mut(recipient_id).simulate_receive_unit(another_recipient_id, another_shard);

    // After receiving 2 shards total (1 at reconstruction + 1 additional),
    // should_receive(2) = true, so the node should emit MessageReceived
    let (recv_publisher, recv_message) = env.node_mut(recipient_id).expect_message_received().await;
    assert_eq!(recv_publisher, publisher_id);
    assert_eq!(recv_message, message);
}

/// Test that a reconstructed unit matches the original unit created by the publisher.
///
/// Delivers a *foreign* shard (via gossip from another recipient) so that reconstruction
/// produces and broadcasts the recipient's own shard — rather than simply forwarding the
/// original unit received from the publisher. The broadcast reconstructed unit should be
/// identical to the original unit the publisher created for that recipient.
#[tokio::test]
async fn test_reconstructed_unit_matches_original() {
    // Setup: 4 nodes (1 publisher + 3 recipients). With 4 nodes,
    // num_data_shards = max(1, (4 - 1) / 3) = 1, so the build threshold is 1 unit and the
    // receive threshold is 2 units. The build threshold is the number of units a node must
    // hold before it can reconstruct the full message and broadcast its own shard; reaching
    // it here after a single received unit is what drives the send counts asserted below.
    let config = Config::default();
    let mut env = TestEnvironment::new(4, config);
    env.simulate_connect_all();
    let committee_id = env.register_committee().await;

    let peer_ids = env.peer_ids();
    let publisher_id = peer_ids[0];
    let reconstructing_recipient = peer_ids[1];
    let gossip_source_recipient = peer_ids[2];

    // Publisher broadcasts a message
    let message = vec![42u8; 100];
    env.node_mut(publisher_id)
        .behaviour
        .broadcast(committee_id, message.clone())
        .await
        .unwrap()
        .unwrap();

    // Collect initial shards from publisher (3 shards to 3 recipients)
    let mut initial_shards = BTreeMap::new();
    for _ in 0..3 {
        let (peer, unit) = env.node_mut(publisher_id).expect_send_unit().await;
        initial_shards.insert(peer, unit);
    }

    // Save the original unit designated for the reconstructing recipient.
    let original_unit = initial_shards.get(&reconstructing_recipient).unwrap().clone();

    let gossip_source_unit = initial_shards.get(&gossip_source_recipient).unwrap().clone();

    // Deliver the gossip source its own shard, triggering its reconstruction and gossip.
    env.node_mut(gossip_source_recipient).simulate_receive_unit(publisher_id, gossip_source_unit);

    // Collect the gossipped unit destined for the reconstructing recipient.
    let mut gossipped_unit = None;
    for _ in 0..2 {
        let (peer, unit) = env.node_mut(gossip_source_recipient).expect_send_unit().await;
        if peer == reconstructing_recipient {
            assert!(
                gossipped_unit.is_none(),
                "Gossip source sent more than one unit to the reconstructing recipient"
            );
            gossipped_unit = Some(unit);
        }
    }
    let gossipped_unit = gossipped_unit.unwrap();

    // Deliver the foreign shard to the reconstructing recipient via gossip. Since this shard
    // carries the gossip source's index rather than the reconstructing recipient's, it must
    // not be forwarded as-is. Instead, reconstruction should trigger, producing and
    // broadcasting the reconstructing recipient's own shard.
    env.node_mut(reconstructing_recipient)
        .simulate_receive_unit(gossip_source_recipient, gossipped_unit);

    // Check that the reconstructing recipient broadcasts its reconstructed shard to the
    // other 2 recipients.
    for _ in 0..2 {
        let (_peer, reconstructed_unit) =
            env.node_mut(reconstructing_recipient).expect_send_unit().await;
        assert_eq!(
            reconstructed_unit, original_unit,
            "Reconstructed unit should match the original unit from the publisher"
        );
    }
}
