use alloy::sol_types::SolEvent;

use crate::ethereum_base_layer_contract::Starknet;

pub type EventIdentifier = &'static str;

pub const LOG_MESSAGE_TO_L2_EVENT_IDENTIFIER: &str = Starknet::LogMessageToL2::SIGNATURE;
pub const CONSUMED_MESSAGE_TO_L2_EVENT_IDENTIFIER: &str = Starknet::ConsumedMessageToL2::SIGNATURE;
pub const MESSAGE_TO_L2_CANCELLATION_STARTED_EVENT_IDENTIFIER: &str =
    Starknet::MessageToL2CancellationStarted::SIGNATURE;
pub const MESSAGE_TO_L2_CANCELED_EVENT_IDENTIFIER: &str = Starknet::MessageToL2Canceled::SIGNATURE;
