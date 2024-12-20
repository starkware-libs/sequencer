use std::sync::Arc;

use alloy_primitives::{Address as EthereumContractAddress, U256};
use alloy_rpc_types_eth::Log;
use alloy_sol_types::SolEventInterface;
use starknet_api::core::{EntryPointSelector, Nonce};
use starknet_api::transaction::fields::Calldata;
use starknet_types_core::felt::Felt;

use crate::ethereum_base_layer_contract::{
    EthereumBaseLayerError,
    EthereumBaseLayerResult,
    Starknet,
};
use crate::{EventData, L1Event};

impl TryFrom<Log> for L1Event {
    type Error = EthereumBaseLayerError;

    fn try_from(log: Log) -> EthereumBaseLayerResult<Self> {
        let validate = true;
        let log = log.inner;

        let event = Starknet::StarknetEvents::decode_log(&log, validate)?.data;
        match event {
            Starknet::StarknetEvents::LogMessageToL2(_event) => {
                todo!()
            }
            Starknet::StarknetEvents::ConsumedMessageToL2(_event) => {
                todo!()
            }
            Starknet::StarknetEvents::MessageToL2Canceled(event) => {
                Ok(L1Event::MessageToL2Canceled(event.try_into()?))
            }
            Starknet::StarknetEvents::MessageToL2CancellationStarted(event) => {
                Ok(L1Event::MessageToL2CancellationStarted(event.try_into()?))
            }
            _ => Err(EthereumBaseLayerError::UnhandledL1Event(log)),
        }
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
