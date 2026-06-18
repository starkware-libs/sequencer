use apollo_network_benchmark::node_args::{Mode, NetworkProtocol, NodeArgs, RunnerArgs, UserArgs};

use super::round_robin_owner_at;

fn make_args(id: u64, num_other_peers: usize, round_duration_seconds: u64) -> NodeArgs {
    NodeArgs {
        runner: RunnerArgs {
            id,
            metric_port: 0,
            p2p_port: 0,
            bootstrap: vec![String::new(); num_other_peers],
        },
        user: UserArgs {
            verbosity: 0,
            buffer_size: 0,
            mode: Mode::RoundRobin,
            network_protocol: NetworkProtocol::Gossipsub,
            broadcaster: None,
            round_duration_seconds,
            message_size_bytes: 0,
            heartbeat_millis: 1,
            timeout_seconds: 0,
        },
    }
}

#[test]
fn zero_round_duration_yields_no_owner() {
    let args = make_args(0, 2, 0);
    assert_eq!(round_robin_owner_at(0, &args), None);
    assert_eq!(round_robin_owner_at(1_234_567, &args), None);
}

#[test]
fn single_node_always_owns_the_round() {
    // bootstrap is empty → num_nodes = 1; owner is always node 0.
    let args = make_args(0, 0, 3);
    for now_seconds in [0u64, 1, 2, 3, 100, 100_000] {
        assert_eq!(round_robin_owner_at(now_seconds, &args), Some(0));
    }
}

#[test]
fn ownership_rotates_across_nodes_at_round_boundaries() {
    // Three nodes, 5-second rounds. The current node id is unused by `round_robin_owner_at`,
    // so we just check the schedule.
    let args = make_args(0, 2, 5);
    assert_eq!(round_robin_owner_at(0, &args), Some(0));
    assert_eq!(round_robin_owner_at(4, &args), Some(0));
    assert_eq!(round_robin_owner_at(5, &args), Some(1));
    assert_eq!(round_robin_owner_at(9, &args), Some(1));
    assert_eq!(round_robin_owner_at(10, &args), Some(2));
    assert_eq!(round_robin_owner_at(14, &args), Some(2));
    // Wraps back to node 0 on the next cycle.
    assert_eq!(round_robin_owner_at(15, &args), Some(0));
}
