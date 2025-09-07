use apollo_mempool_p2p::metrics::{
    MEMPOOL_P2P_BROADCASTED_BATCH_SIZE,
    MEMPOOL_P2P_NUM_CONNECTED_PEERS,
    MEMPOOL_P2P_NUM_RECEIVED_MESSAGES,
    MEMPOOL_P2P_NUM_SENT_MESSAGES,
};

use crate::dashboard::{Panel, Row};

fn get_panel_mempool_p2p_num_connected_peers() -> Panel {
    Panel::from(&MEMPOOL_P2P_NUM_CONNECTED_PEERS)
}

fn get_panel_mempool_p2p_num_sent_messages() -> Panel {
    Panel::from(&MEMPOOL_P2P_NUM_SENT_MESSAGES)
}

fn get_panel_mempool_p2p_num_received_messages() -> Panel {
    Panel::from(&MEMPOOL_P2P_NUM_RECEIVED_MESSAGES)
}

fn get_panel_mempool_p2p_broadcasted_batch_size() -> Panel {
    Panel::from(&MEMPOOL_P2P_BROADCASTED_BATCH_SIZE)
}

pub(crate) fn get_mempool_p2p_row() -> Row {
    Row::new(
        "MempoolP2p",
        vec![
            get_panel_mempool_p2p_num_connected_peers(),
            get_panel_mempool_p2p_num_sent_messages(),
            get_panel_mempool_p2p_num_received_messages(),
            get_panel_mempool_p2p_broadcasted_batch_size(),
        ],
    )
}
