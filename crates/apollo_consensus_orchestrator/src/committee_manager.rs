use std::sync::Arc;

use blockifier::context::BlockContext;
use blockifier::execution::call_info::Retdata;
use blockifier::execution::entry_point::call_view_entry_point;
use blockifier::execution::errors::EntryPointExecutionError;
use blockifier::state::state_api::StateReader;
use starknet_api::core::ContractAddress;
use starknet_api::felt;
use starknet_api::transaction::fields::Calldata;
use starknet_types_core::felt::Felt;
use thiserror::Error;

#[cfg(test)]
#[path = "committee_manager_test.rs"]
mod committee_manager_test;

const STAKER_CAIRO_OBJECT_LENGTH: usize = 3;

pub struct CommitteeManagerConfig {
    pub staking_contract_address: ContractAddress,
}

// Responsible for fetching and storing the committee at a given epoch.
// The committee is the list of stakers that participate in the consensus at a given epoch.
pub struct CommitteeManager {
    #[allow(dead_code)]
    config: CommitteeManagerConfig,
}

#[derive(Debug, Error)]
pub enum CommitteeManagerError {
    #[error(transparent)]
    EntryPointExecutionError(#[from] EntryPointExecutionError),
    #[error(transparent)]
    RetdataDeserializationError(#[from] RetdataDeserializationError),
}

#[derive(Debug, Error)]
pub enum RetdataDeserializationError {
    #[error("Failed to convert Felt to ContractAddress: {address}")]
    ContractAddressConversionError { address: Felt },
    #[error("Failed to convert Felt to u128: {felt}")]
    U128ConversionError { felt: Felt },
    #[error("Invalid retdata length: {length}")]
    InvalidRetdataLength { length: usize },
}

pub type CommitteeManagerResult<T> = Result<T, CommitteeManagerError>;

impl CommitteeManager {
    pub fn new(config: CommitteeManagerConfig) -> Self {
        Self { config }
    }

    // Returns a list of the committee members at the given epoch.
    // The state's most recent block should be provided in the block_context.
    pub fn get_committee_at_epoch(
        &self,
        epoch: u64,
        state_reader: impl StateReader,
        block_context: Arc<BlockContext>,
    ) -> CommitteeManagerResult<Vec<Staker>> {
        let call_info = call_view_entry_point(
            state_reader,
            block_context,
            self.config.staking_contract_address,
            "get_stakers",
            Calldata(vec![felt!(epoch)].into()),
        )?;

        let stakers = ArrayRetdata::<STAKER_CAIRO_OBJECT_LENGTH, Staker>::try_from(
            call_info.execution.retdata,
        )?;

        Ok(stakers.0)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Staker {
    // A contract address of the staker, to which rewards are sent.
    pub address: ContractAddress,
    // The amount of tokens staked by the staker.
    pub staked_amount: u128,
    // The public key of the staker, used to verify the staker's identity.
    pub pubkey: Felt,
}

impl TryFrom<[Felt; STAKER_CAIRO_OBJECT_LENGTH]> for Staker {
    type Error = RetdataDeserializationError;

    fn try_from(felts: [Felt; STAKER_CAIRO_OBJECT_LENGTH]) -> Result<Self, Self::Error> {
        let [address, staked_amount, pubkey] = felts;
        let address = ContractAddress::try_from(address)
            .map_err(|_| RetdataDeserializationError::ContractAddressConversionError { address })?;
        let staked_amount = u128::try_from(staked_amount).map_err(|_| {
            RetdataDeserializationError::U128ConversionError { felt: staked_amount }
        })?;
        Ok(Self { address, staked_amount, pubkey })
    }
}

#[cfg(test)]
impl From<&Staker> for Vec<Felt> {
    fn from(staker: &Staker) -> Self {
        vec![Felt::from(staker.address), Felt::from(staker.staked_amount), staker.pubkey]
    }
}

// A representation of a Cairo1 `Array` of elements that can be deserialized to T.
// T must be convertible from an array of N Felts.
#[derive(Debug, PartialEq, Eq)]
struct ArrayRetdata<const N: usize, T>(Vec<T>);

impl<const N: usize, T> TryFrom<Retdata> for ArrayRetdata<N, T>
where
    T: TryFrom<[Felt; N], Error = RetdataDeserializationError>,
{
    type Error = RetdataDeserializationError;

    fn try_from(retdata: Retdata) -> Result<Self, Self::Error> {
        let data = retdata.0;

        // The first Felt in the Retdata must be the number of elements in the array.
        if data.is_empty() {
            return Err(RetdataDeserializationError::InvalidRetdataLength { length: data.len() });
        }

        // Split the remaining Felts into chunks of N Felts, each is an element of the array.
        let data_chunks = data[1..].chunks_exact(N);

        // Verify that the number of elements in the array matches the number of chunks.
        let num_elements = usize::try_from(data[0]).expect("num_elements should fit in usize.");
        if data_chunks.len() != num_elements || !data_chunks.remainder().is_empty() {
            return Err(RetdataDeserializationError::InvalidRetdataLength { length: data.len() });
        }

        // Convert each element to T.
        let result = data_chunks
            .map(|chunk| T::try_from(chunk.try_into().expect("chunk size must be N.")))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ArrayRetdata(result))
    }
}
