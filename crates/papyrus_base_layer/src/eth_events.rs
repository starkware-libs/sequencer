use std::sync::Arc;

use alloy_primitives::{Address as EthereumContractAddress, U256};
use alloy_rpc_types_eth::Log;
use alloy_sol_types::SolEvent;
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
        let event_signature =
            *log.topic0().ok_or_else(|| Self::Error::UnhandledL1Event(log.inner.clone()))?;
        let validate = true;
        let log = log.inner;

        Ok(match event_signature {
            sig if sig == Starknet::LogMessageToL2::SIGNATURE_HASH => {
                todo!()
            }
            sig if sig == Starknet::MessageToL2CancellationStarted::SIGNATURE_HASH => {
                let decoded = Starknet::MessageToL2CancellationStarted::decode_log(&log, validate)?;
                L1Event::MessageToL2CancellationStarted(decoded.try_into()?)
            }
            sig if sig == Starknet::MessageToL2Canceled::SIGNATURE_HASH => {
                todo!()
            }
            sig if sig == Starknet::ConsumedMessageToL1::SIGNATURE_HASH => {
                todo!()
            }
            _ => return Err(EthereumBaseLayerError::UnhandledL1Event(log)),
        })
    }
}

impl TryFrom<alloy_primitives::Log<Starknet::MessageToL2CancellationStarted>> for EventData {
    type Error = EthereumBaseLayerError;

    fn try_from(
        decoded: alloy_primitives::Log<Starknet::MessageToL2CancellationStarted>,
    ) -> EthereumBaseLayerResult<Self> {
        create_l1_message_data(
            decoded.fromAddress,
            decoded.toAddress,
            decoded.selector,
            &decoded.payload,
            decoded.nonce,
        )
    }
}

pub fn create_l1_message_data(
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
