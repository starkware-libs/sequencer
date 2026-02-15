use apollo_metrics::metrics::MetricQueryName;
use apollo_staking::metrics::{
    STAKING_COMMITTEE_ELIGIBLE_PROPOSERS_TOTAL_WEIGHT,
    STAKING_COMMITTEE_TOTAL_WEIGHT,
    STAKING_CURRENT_EPOCH_ID,
    STAKING_CURRENT_EPOCH_LENGTH,
    STAKING_CURRENT_EPOCH_START_BLOCK,
};

use crate::dashboard::Row;
use crate::panel::{Panel, PanelType};

fn get_panel_epoch_info() -> Panel {
    Panel::new(
        "Epoch Information",
        "Current epoch ID, current epoch start height, and next epoch start height",
        vec![
            STAKING_CURRENT_EPOCH_ID.get_name_with_filter().to_string(),
            STAKING_CURRENT_EPOCH_START_BLOCK.get_name_with_filter().to_string(),
            format!(
                "{} + {}",
                STAKING_CURRENT_EPOCH_START_BLOCK.get_name_with_filter(),
                STAKING_CURRENT_EPOCH_LENGTH.get_name_with_filter()
            ),
        ],
        PanelType::Stat,
    )
    .with_legends(vec!["Epoch ID", "Start Height", "Next Epoch Start Height"])
}

fn get_panel_stake_distribution() -> Panel {
    Panel::new(
        "Stake Distribution by Staker",
        "The distribution of stake across committee members, showing each staker's address and \
         their staking power",
        "staking_committee_member_weight{cluster=~\"$cluster\", namespace=~\"$namespace\", \
         pod=~\"$pod\"}"
            .to_string(),
        PanelType::PieChart,
    )
}

fn get_panel_eligible_proposers_stake_percentage() -> Panel {
    Panel::new(
        "Eligible Proposers Stake %",
        "The percentage of total stake held by eligible proposers",
        format!(
            "({} / {}) * 100",
            STAKING_COMMITTEE_ELIGIBLE_PROPOSERS_TOTAL_WEIGHT.get_name_with_filter(),
            STAKING_COMMITTEE_TOTAL_WEIGHT.get_name_with_filter()
        ),
        PanelType::Stat,
    )
}

pub(crate) fn get_staking_row() -> Row {
    Row::new(
        "Staking",
        vec![
            get_panel_epoch_info(),
            get_panel_stake_distribution(),
            get_panel_eligible_proposers_stake_percentage(),
        ],
    )
}
