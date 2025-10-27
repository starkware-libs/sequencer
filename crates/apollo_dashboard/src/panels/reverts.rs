use apollo_consensus_manager::metrics::CONSENSUS_REVERTED_BATCHER_UP_TO_AND_INCLUDING;
use apollo_state_sync_metrics::metrics::STATE_SYNC_REVERTED_UP_TO_AND_INCLUDING;

use crate::dashboard::{Panel, PanelType, Row};

fn get_panel_consensus_reverts() -> Panel {
    #[allow(deprecated)]
    Panel::from_gauge(&CONSENSUS_REVERTED_BATCHER_UP_TO_AND_INCLUDING, PanelType::Stat)
}

fn get_panel_state_sync_reverts() -> Panel {
    #[allow(deprecated)]
    Panel::from_gauge(&STATE_SYNC_REVERTED_UP_TO_AND_INCLUDING, PanelType::Stat)
}

pub(crate) fn get_reverts_row() -> Row {
    Row::new("Reverts", vec![get_panel_consensus_reverts(), get_panel_state_sync_reverts()])
}
