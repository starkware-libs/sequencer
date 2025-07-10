use std::collections::BTreeMap;
use std::sync::Arc;

use apollo_consensus::types::Round;
use apollo_state_sync_types::communication::SharedStateSyncClient;
use blockifier::context::BlockContext;
use blockifier::execution::call_info::Retdata;
use blockifier::execution::entry_point::call_view_entry_point;
use blockifier::execution::errors::EntryPointExecutionError;
use blockifier::state::state_api::StateReader;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::ContractAddress;
use starknet_api::staking::StakingWeight;
use starknet_api::transaction::fields::Calldata;
use starknet_types_core::felt::Felt;
use thiserror::Error;

pub type Committee = Vec<Staker>;
pub type StakerSet = Vec<Staker>;

const GET_STAKERS_ENTRY_POINT: &str = "get_stakers";
const EPOCH_LENGTH: u64 = 100; // Number of heights in an epoch.

#[cfg(test)]
#[path = "committee_manager_test.rs"]
mod committee_manager_test;

// Holds committee data for the highest known epochs, limited in size by `capacity``.
struct CommitteeDataCache {
    // The maximum number of epochs to cache.
    capacity: usize,
    // A map of epoch to the epoch's data.
    cache: BTreeMap<u64, Arc<Committee>>,
}

impl CommitteeDataCache {
    pub fn new(capacity: usize) -> Self {
        Self { capacity, cache: BTreeMap::new() }
    }

    pub fn get(&self, epoch: u64) -> Option<&Arc<Committee>> {
        self.cache.get(&epoch)
    }

    pub fn insert(&mut self, epoch: u64, data: Arc<Committee>) {
        self.cache.insert(epoch, data);
        if self.cache.len() > self.capacity {
            self.cache.pop_first();
        }
    }
}

pub struct CommitteeManagerConfig {
    pub staking_contract_address: ContractAddress,
    pub max_cached_epochs: usize,

    // The desired number of committee members to select from the available stakers.
    // If there are fewer stakers than `committee_size`, a smaller committee will be selected.
    pub committee_size: usize,

    // Defines how many heights in advance the proposer can be predicted.
    // While the exact identity may depend on staker prediction constraints,
    // the proposer selection logic becomes deterministic at this offset.
    pub proposer_prediction_window_in_heights: u64,
}

// Responsible for fetching and storing the committee at a given epoch.
// The committee is a subset of nodes (proposer and validators) that are selected to participate in
// the consensus at a given epoch, responsible for proposing blocks and voting on them.
pub struct CommitteeManager {
    committee_data_cache: CommitteeDataCache,
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
    #[error(
        "Invalid retdata length: expected 1 Felt followed by {num_structs} (number of structs) *
         {struct_size} (number of Felts per struct), but received {length} Felts."
    )]
    InvalidArrayLength { length: usize, num_structs: usize, struct_size: usize },
}

#[cfg_attr(test, derive(Clone))]
pub struct ExecutionContext<S: StateReader> {
    pub state_reader: S,
    pub block_context: Arc<BlockContext>,
    pub state_sync_client: SharedStateSyncClient,
}

pub type CommitteeManagerResult<T> = Result<T, CommitteeManagerError>;

impl CommitteeManager {
    pub fn new(config: CommitteeManagerConfig) -> Self {
        Self { committee_data_cache: CommitteeDataCache::new(config.max_cached_epochs), config }
    }

    // Returns a list of the committee members at the given epoch.
    // The state's most recent block should be provided in the block_context.
    pub fn get_committee<S: StateReader>(
        &mut self,
        epoch: u64,
        execution_context: ExecutionContext<S>,
    ) -> CommitteeManagerResult<Arc<Committee>> {
        if let Some(committee_data) = self.committee_data_cache.get(epoch) {
            return Ok(committee_data.clone());
        }

        let call_info = call_view_entry_point(
            execution_context.state_reader,
            execution_context.block_context,
            self.config.staking_contract_address,
            GET_STAKERS_ENTRY_POINT,
            Calldata(vec![Felt::from(epoch)].into()),
        )?;

        let stakers = Staker::from_retdata_many(call_info.execution.retdata)?;

        let committee = Arc::new(self.select_committee(stakers));
        self.committee_data_cache.insert(epoch, committee.clone());

        Ok(committee)
    }

    // Returns the address of the proposer for the specified height and round.
    //
    // The proposer is chosen from the committee corresponding to the epoch of the given height.
    // Selection is based on a deterministic random number derived from the height, round,
    // and the hash of a past block — offset by `config.proposer_prediction_window`.
    pub async fn get_proposer<S: StateReader>(
        &mut self,
        height: BlockNumber,
        round: Round,
        execution_context: ExecutionContext<S>,
    ) -> CommitteeManagerResult<ContractAddress> {
        // Try to get the hash of the block used for proposer selection randomness.
        let block_hash = self
            .proposer_randomness_block_hash(height, execution_context.state_sync_client.clone())
            .await?;

        // Generate a pseudorandom value based on the height, round, and block hash.
        let random_value = get_pseudorandom_value(height, round, block_hash);

        // Get the committee for the epoch this height belongs to.
        let epoch = height.0 / EPOCH_LENGTH; // TODO(Dafna): export to a utility function.
        let committee = self.get_committee(epoch, execution_context)?;

        // Select a proposer from the committee using the generated random.
        let proposer = self.choose_proposer(&committee, random_value);
        Ok(proposer.address)
    }

    fn select_committee(&self, mut stakers: StakerSet) -> Committee {
        // Take the top `committee_size` stakers by weight.
        stakers.sort_by_key(|staker| staker.weight);
        stakers.into_iter().rev().take(self.config.committee_size).collect()
    }

    async fn proposer_randomness_block_hash(
        &self,
        _block_number: BlockNumber,
        _state_sync_client: SharedStateSyncClient,
    ) -> CommitteeManagerResult<Option<BlockHash>> {
        todo!()
    }

    fn choose_proposer(&self, _committee: &Committee, _random: u64) -> &Staker {
        todo!()
    }
}

fn get_pseudorandom_value(
    _height: BlockNumber,
    _round: Round,
    _block_hash: Option<BlockHash>,
) -> u64 {
    todo!()
}

#[cfg_attr(test, derive(Clone))]
#[derive(Debug, PartialEq, Eq)]
pub struct Staker {
    // A contract address of the staker, to which rewards are sent.
    pub address: ContractAddress,
    // The staker's weight, which determines the staker's influence in the consensus (its voting
    // power).
    pub weight: StakingWeight,
    // The public key of the staker, used to verify the staker's identity.
    pub public_key: Felt,
}

impl Staker {
    pub const CAIRO_OBJECT_NUM_FELTS: usize = 3;

    pub fn from_retdata_many(retdata: Retdata) -> Result<Vec<Self>, RetdataDeserializationError> {
        Ok(ArrayRetdata::<{ Self::CAIRO_OBJECT_NUM_FELTS }, Staker>::try_from(retdata)?.0)
    }
}

impl TryFrom<[Felt; Self::CAIRO_OBJECT_NUM_FELTS]> for Staker {
    type Error = RetdataDeserializationError;

    fn try_from(felts: [Felt; Self::CAIRO_OBJECT_NUM_FELTS]) -> Result<Self, Self::Error> {
        let [address, weight, public_key] = felts;
        let address = ContractAddress::try_from(address)
            .map_err(|_| RetdataDeserializationError::ContractAddressConversionError { address })?;
        let weight = StakingWeight(
            u128::try_from(weight)
                .map_err(|_| RetdataDeserializationError::U128ConversionError { felt: weight })?,
        );
        Ok(Self { address, weight, public_key })
    }
}

#[cfg(test)]
impl From<&Staker> for Vec<Felt> {
    fn from(staker: &Staker) -> Self {
        vec![Felt::from(staker.address), Felt::from(staker.weight.0), staker.public_key]
    }
}

// Represents a Cairo1 `Array` containing elements that can be deserialized to `T`.
// `T` must implement `TryFrom<[Felt; N]>`, where `N` is the size of `T`'s Cairo equivalent.
#[derive(Debug, PartialEq, Eq)]
struct ArrayRetdata<const N: usize, T>(Vec<T>);

impl<const N: usize, T> TryFrom<Retdata> for ArrayRetdata<N, T>
where
    T: TryFrom<[Felt; N], Error = RetdataDeserializationError>,
{
    type Error = RetdataDeserializationError;

    fn try_from(retdata: Retdata) -> Result<Self, Self::Error> {
        let data = retdata.0;

        // The first Felt in the Retdata must be the number of structs in the array.
        if data.is_empty() {
            return Err(RetdataDeserializationError::InvalidArrayLength {
                length: data.len(),
                num_structs: 0,
                struct_size: N,
            });
        }

        // Split the remaining Felts into chunks of N Felts, each is a struct in the array.
        let data_chunks = data[1..].chunks_exact(N);

        // Verify that the number of structs in the array matches the number of chunks.
        let num_structs = usize::try_from(data[0]).expect("num_structs should fit in usize.");
        if data_chunks.len() != num_structs || !data_chunks.remainder().is_empty() {
            return Err(RetdataDeserializationError::InvalidArrayLength {
                length: data.len(),
                num_structs,
                struct_size: N,
            });
        }

        // Convert each chunk to T.
        let result = data_chunks
            .map(|chunk| {
                T::try_from(
                    chunk.try_into().unwrap_or_else(|_| panic!("chunk size must be N: {N}.")),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ArrayRetdata(result))
    }
}
