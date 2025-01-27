use crate::dashboard::{Dashboard, Panel, PanelType, Row};
use crate::metric_definitions::{
    ADDED_TRANSACTIONS_TOTAL,
    BATCHED_TRANSACTIONS,
    PROPOSAL_FAILED,
    PROPOSAL_STARTED,
    PROPOSAL_SUCCEEDED,
};

const PANEL_ADDED_TRANSACTIONS_TOTAL: Panel = Panel::new(
    ADDED_TRANSACTIONS_TOTAL.get_name(),
    ADDED_TRANSACTIONS_TOTAL.get_description(),
    ADDED_TRANSACTIONS_TOTAL.get_name(),
    PanelType::Stat,
);

const PANEL_PROPOSAL_STARTED: Panel = Panel::new(
    PROPOSAL_STARTED.get_name(),
    PROPOSAL_STARTED.get_description(),
    PROPOSAL_STARTED.get_name(),
    PanelType::Stat,
);
const PANEL_PROPOSAL_SUCCEEDED: Panel = Panel::new(
    PROPOSAL_SUCCEEDED.get_name(),
    PROPOSAL_SUCCEEDED.get_description(),
    PROPOSAL_SUCCEEDED.get_name(),
    PanelType::Stat,
);
const PANEL_PROPOSAL_FAILED: Panel = Panel::new(
    PROPOSAL_FAILED.get_name(),
    PROPOSAL_FAILED.get_description(),
    PROPOSAL_FAILED.get_name(),
    PanelType::Stat,
);
const PANEL_BATCHED_TRANSACTIONS: Panel = Panel::new(
    BATCHED_TRANSACTIONS.get_name(),
    BATCHED_TRANSACTIONS.get_description(),
    BATCHED_TRANSACTIONS.get_name(),
    PanelType::Stat,
);

const BATCHER_ROW: Row<'_> = Row::new(
    "Batcher",
    "Batcher metrics including proposals and transactions",
    &[
        PANEL_PROPOSAL_STARTED,
        PANEL_PROPOSAL_SUCCEEDED,
        PANEL_PROPOSAL_FAILED,
        PANEL_BATCHED_TRANSACTIONS,
    ],
);
const HTTP_SERVER_ROW: Row<'_> = Row::new(
    "Http Server",
    "Http Server metrics including added transactions",
    &[PANEL_ADDED_TRANSACTIONS_TOTAL],
);

pub const SEQUENCER_DASHBOARD: Dashboard<'_> = Dashboard::new(
    "Sequencer Node Dashboard",
    "Monitoring of the decentralized sequencer node",
    &[BATCHER_ROW, HTTP_SERVER_ROW],
);
