use std::sync::Arc;

use alloy::primitives::{Address as EthereumContractAddress, U256};
use alloy::rpc::types::Log;
use alloy::sol_types::SolEventInterface;
use starknet_api::block::BlockTimestamp;
use starknet_api::core::{EntryPointSelector, Nonce};
use starknet_api::transaction::fields::{Calldata, Fee};
use starknet_api::transaction::L1HandlerTransaction;
use starknet_types_core::felt::Felt;

use crate::ethereum_base_layer_contract::{
    EthereumBaseLayerError,
    EthereumBaseLayerResult,
    Starknet,
};
use crate::{EventData, L1Event};

// Note: don't move as method for L1Event, we don't want to expose alloy's inner type Log to our
// base layer's API.
pub fn parse_event(log: Log, block_timestamp: BlockTimestamp) -> EthereumBaseLayerResult<L1Event> {
    let validate = true;
    let l1_tx_hash = log.transaction_hash;
    let log = log.inner;

    let event = Starknet::StarknetEvents::decode_log(&log, validate)?.data;
    match event {
        Starknet::StarknetEvents::LogMessageToL2(event) => {
            let fee = Fee(event.fee.try_into().map_err(EthereumBaseLayerError::FeeOutOfRange)?);
            let event_data = EventData::try_from(event)?;
            let tx = L1HandlerTransaction::from(event_data);
            Ok(L1Event::LogMessageToL2 { tx, fee, l1_tx_hash, block_timestamp })
        }
        Starknet::StarknetEvents::ConsumedMessageToL2(event) => {
            let event_data = EventData::try_from(event)?;
            let tx = L1HandlerTransaction::from(event_data);
            Ok(L1Event::ConsumedMessageToL2(tx))
        }
        Starknet::StarknetEvents::MessageToL2Canceled(event) => {
            Ok(L1Event::MessageToL2Canceled(event.try_into()?))
        }
        Starknet::StarknetEvents::MessageToL2CancellationStarted(event) => {
            Ok(L1Event::MessageToL2CancellationStarted {
                cancelled_tx: EventData::try_from(event)?.into(),
                cancellation_request_timestamp: block_timestamp,
            })
        }
        _ => Err(EthereumBaseLayerError::UnhandledL1Event(log)),
    }
}

impl TryFrom<Starknet::MessageToL2Canceled> for EventData {
    type Error = EthereumBaseLayerError;

    fn try_from(event: Starknet::MessageToL2Canceled) -> EthereumBaseLayerResult<Self> {
        create_l1_event_data(
            event.fromAddress,
            event.toAddress,
            event.selector,
            &event.payload,
            event.nonce,
        )
    }
}

impl TryFrom<Starknet::MessageToL2CancellationStarted> for EventData {
    type Error = EthereumBaseLayerError;

    fn try_from(event: Starknet::MessageToL2CancellationStarted) -> EthereumBaseLayerResult<Self> {
        create_l1_event_data(
            event.fromAddress,
            event.toAddress,
            event.selector,
            &event.payload,
            event.nonce,
        )
    }
}

impl TryFrom<Starknet::LogMessageToL2> for EventData {
    type Error = EthereumBaseLayerError;

    fn try_from(decoded: Starknet::LogMessageToL2) -> EthereumBaseLayerResult<Self> {
        create_l1_event_data(
            decoded.fromAddress,
            decoded.toAddress,
            decoded.selector,
            &decoded.payload,
            decoded.nonce,
        )
    }
}

impl TryFrom<Starknet::ConsumedMessageToL2> for EventData {
    type Error = EthereumBaseLayerError;

    fn try_from(event: Starknet::ConsumedMessageToL2) -> EthereumBaseLayerResult<Self> {
        create_l1_event_data(
            event.fromAddress,
            event.toAddress,
            event.selector,
            &event.payload,
            event.nonce,
        )
    }
}

pub fn create_l1_event_data(
    from_address: EthereumContractAddress,
    to_address: U256,
    selector: U256,
    payload: &[U256],
    nonce: U256,
) -> EthereumBaseLayerResult<EventData> {
    Ok(EventData {
        from_address: Felt::from_bytes_be_slice(from_address.0.as_slice())
            .try_into()
            .map_err(EthereumBaseLayerError::StarknetApiParsingError)?,
        to_address: felt_from_u256(to_address)
            .try_into()
            .map_err(EthereumBaseLayerError::StarknetApiParsingError)?,
        entry_point_selector: EntryPointSelector(felt_from_u256(selector)),
        payload: Calldata(Arc::new(payload.iter().map(|&x| felt_from_u256(x)).collect())),
        nonce: Nonce(felt_from_u256(nonce)),
    })
}

pub fn felt_from_u256(num: U256) -> Felt {
    Felt::from_bytes_be(&num.to_be_bytes())
}
