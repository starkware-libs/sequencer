use std::iter::once;
use std::sync::Arc;

use alloy_primitives::{Address as EthereumContractAddress, U256};
use alloy_rpc_types_eth::Log;
use alloy_sol_types::SolEventInterface;
use starknet_api::core::{EntryPointSelector, Nonce};
use starknet_api::hash::StarkHash;
use starknet_api::transaction::fields::{Calldata, Fee};
use starknet_api::transaction::{L1HandlerTransaction, TransactionVersion};
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
            Starknet::StarknetEvents::LogMessageToL2(event) => {
                let fee =
                    Fee(event.fee.try_into().map_err(EthereumBaseLayerError::FeeOutOfRange)?);
                let event_data = EventData::try_from(event)?;
                // Prepend the L1 sender address to the calldata.
                let calldata = Calldata(Arc::new(
                    once(event_data.from_address.into())
                        .chain(Arc::try_unwrap(event_data.payload.0).unwrap())
                        .collect(),
                ));

                const DEFAULT_L1_HANDLER_VERSION: TransactionVersion =
                    TransactionVersion(StarkHash::ZERO);
                let tx = L1HandlerTransaction {
                    version: DEFAULT_L1_HANDLER_VERSION,
                    contract_address: event_data.to_address,
                    entry_point_selector: event_data.entry_point_selector,
                    nonce: event_data.nonce,
                    calldata,
                };
                Ok(L1Event::LogMessageToL2 { tx, fee })
            }
            Starknet::StarknetEvents::ConsumedMessageToL2(event) => {
                Ok(L1Event::ConsumedMessageToL2(event.try_into()?))
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

macro_rules! impl_event_data_conversion {
    ($event_type:ty) => {
        impl TryFrom<$event_type> for EventData {
            type Error = EthereumBaseLayerError;

            fn try_from(event: $event_type) -> EthereumBaseLayerResult<Self> {
                create_l1_event_data(
                    event.fromAddress,
                    event.toAddress,
                    event.selector,
                    &event.payload,
                    event.nonce,
                )
            }
        }
    };
}

impl_event_data_conversion!(Starknet::MessageToL2Canceled);
impl_event_data_conversion!(Starknet::MessageToL2CancellationStarted);
impl_event_data_conversion!(Starknet::LogMessageToL2);
impl_event_data_conversion!(Starknet::ConsumedMessageToL2);

pub fn create_l1_event_data(
    from_address: EthereumContractAddress,
    to_address: U256,
    selector: U256,
    payload: &[U256],
    nonce: U256,
) -> EthereumBaseLayerResult<EventData> {
    Ok(EventData {
        from_address: felt_from_eth_address(from_address)
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

pub fn felt_from_eth_address(address: EthereumContractAddress) -> Felt {
    Felt::from_bytes_be_slice(address.0.as_slice())
}

pub fn felt_from_u256(num: U256) -> Felt {
    Felt::from_bytes_be(&num.to_be_bytes())
}
