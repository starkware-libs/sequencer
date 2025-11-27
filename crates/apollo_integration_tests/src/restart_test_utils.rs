use std::collections::{HashMap, HashSet};
use std::time::Duration;

use tokio::time::{sleep, Instant};
use tracing::info;

use crate::integration_test_manager::IntegrationTestManager;
use crate::monitoring_utils::get_consensus_decisions_reached;

// Verifies that all running nodes processed more transactions since the last check.
// Takes a mutable reference to a mapping of the number of transactions processed by each node
// at the previous check, and updates it with the current number of transactions.
pub async fn poll_running_nodes_received_more_txs(
    curr_txs: &mut HashMap<usize, usize>,
    integration_test_manager: &IntegrationTestManager,
    timeout: Duration,
    node_indices: HashSet<usize>,
) {
    let prev_txs = curr_txs.clone();

    let start = Instant::now();
    let mut done: bool = false;
    while start.elapsed() < timeout && !done {
        sleep(Duration::from_secs(1)).await;
        done = update_processed_txs(integration_test_manager, curr_txs, &prev_txs, &node_indices)
            .await;
    }

    info!(
        "Verifying running nodes received more transactions finished in {} seconds",
        start.elapsed().as_secs()
    );
    for node_idx in &node_indices {
        let curr_n_processed = curr_txs.get(node_idx).expect("Num txs not found");
        let prev_n_processed = prev_txs.get(node_idx).expect("Num txs not found");
        info!(
            "Node {} processed {} -> {} transactions",
            node_idx, prev_n_processed, curr_n_processed
        );
    }

    if !done {
        panic!(
            "Not all specified running nodes processed more transactions in the last {} seconds",
            timeout.as_secs()
        );
    }
}

pub async fn poll_all_running_nodes_received_more_txs(
    curr_txs: &mut HashMap<usize, usize>,
    integration_test_manager: &IntegrationTestManager,
    timeout: Duration,
) {
    let node_indices = integration_test_manager.get_running_node_indices();
    poll_running_nodes_received_more_txs(curr_txs, integration_test_manager, timeout, node_indices)
        .await;
}

// Checks if all specified running nodes have processed more transactions than before and updates
// the current transactions mapping with the latest values. Returns true if every specified running
// node's transaction count has increased, and false otherwise.
async fn update_processed_txs(
    integration_test_manager: &IntegrationTestManager,
    curr_processed_txs: &mut HashMap<usize, usize>,
    prev_txs: &HashMap<usize, usize>,
    node_indices: &HashSet<usize>,
) -> bool {
    let curr_txs =
        integration_test_manager.get_num_accepted_txs_on_running_nodes(node_indices.clone()).await;
    for (node_idx, curr_n_processed) in curr_txs {
        curr_processed_txs.insert(node_idx, curr_n_processed).expect("Num txs not found");

        let prev_n_processed =
            prev_txs.get(&node_idx).expect("Node index not found in previous transactions mapping");

        if curr_n_processed <= *prev_n_processed {
            return false;
        }
    }
    true
}

// TODO(lev): Make a polling function that receives as args a condition function and a timeout.
// Then we can use it in both poll_... functions.

/// Verifies that the node with the given index reaches consensus decisions after being restarted.
pub async fn poll_node_reaches_consensus_decisions_after_restart(
    integration_test_manager: &IntegrationTestManager,
    node_idx: usize,
    timeout: Duration,
) {
    let consensus_monitoring_client = integration_test_manager
        .get_consensus_manager_monitoring_client_for_running_node(node_idx)
        .await;
    let prev_decisions_reached = get_consensus_decisions_reached(consensus_monitoring_client).await;
    let mut curr_decisions_reached = prev_decisions_reached;
    let start = Instant::now();
    while start.elapsed() < timeout {
        sleep(Duration::from_secs(1)).await;
        // TODO(lev): We should use a more efficient metrics to be sure that restarted node
        // integrated back in reaching consensus.
        curr_decisions_reached = get_consensus_decisions_reached(consensus_monitoring_client).await;
        if curr_decisions_reached > prev_decisions_reached + 1 {
            info!(
                "Verifying node is reaching consensus decisions after restart finished in {} \
                 seconds",
                start.elapsed().as_secs()
            );
            return;
        }
    }
    info!(
        "Consensus decisions reached: previous - {}, current - {}",
        prev_decisions_reached, curr_decisions_reached
    );
    panic!(
        "Node {node_idx} did not reach consensus decisions after restart in the last {} seconds",
        timeout.as_secs()
    );
}
