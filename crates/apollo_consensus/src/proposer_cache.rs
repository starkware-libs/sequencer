//! Proposer cache to avoid blocking on proposer/virtual_proposer lookups.
//!
//! Prefetches proposers asynchronously; lookups are sync reads from the cache.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use starknet_api::block::BlockNumber;

use crate::types::{ConsensusContext, ConsensusError, Round, ValidatorId};

/// Sync lookup for proposer and virtual proposer by round.
/// Implemented by [`ProposerCache`] in production; mocked in tests.
pub trait ProposerLookup: Send + Sync {
    /// Returns the actual proposer for the round, or error if not cached.
    fn actual_proposer(&self, round: Round) -> Result<ValidatorId, ConsensusError>;
    /// Returns the virtual proposer for the round, or error if not cached.
    fn virtual_proposer(&self, round: Round) -> Result<ValidatorId, ConsensusError>;
}

#[derive(Debug, Default)]
struct CacheData {
    height: BlockNumber,
    proposers: HashMap<Round, ValidatorId>,
    virtual_proposers: HashMap<Round, ValidatorId>,
    last_round_fetched: u32,
}

#[derive(Debug)]
pub struct ProposerCache {
    data: RwLock<CacheData>,
}

impl ProposerLookup for ProposerCache {
    fn actual_proposer(&self, round: Round) -> Result<ValidatorId, ConsensusError> {
        self.data.read().unwrap().proposers.get(&round).copied().ok_or_else(|| {
            ConsensusError::InternalNetworkError(format!("Proposer not in cache for round {round}"))
        })
    }

    fn virtual_proposer(&self, round: Round) -> Result<ValidatorId, ConsensusError> {
        self.data.read().unwrap().virtual_proposers.get(&round).copied().ok_or_else(|| {
            ConsensusError::InternalNetworkError(format!(
                "Virtual proposer not in cache for round {round}"
            ))
        })
    }
}

impl ProposerCache {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { data: RwLock::new(CacheData::default()) })
    }

    /// Initialize for a height: set height, clear previous data, prefetch first 5 rounds.
    /// Call before creating SHC for each height.
    pub async fn init<C: ConsensusContext>(
        &self,
        height: BlockNumber,
        context: &mut C,
    ) -> Result<(), ConsensusError> {
        {
            let mut d = self.data.write().unwrap();
            d.height = height;
            d.proposers.clear();
            d.virtual_proposers.clear();
        }
        let mut proposers = HashMap::new();
        let mut virtual_proposers = HashMap::new();
        for r in 0..5 {
            let round: Round = r;
            proposers.insert(round, context.proposer(height, round).await?);
            virtual_proposers.insert(round, context.virtual_proposer(height, round).await?);
        }
        {
            let mut d = self.data.write().unwrap();
            d.proposers.extend(proposers);
            d.virtual_proposers.extend(virtual_proposers);
            d.last_round_fetched = 4;
        }
        Ok(())
    }

    /// Notify the cache that a round was seen and prefetch if needed.
    /// Fetches next 5 rounds if round > last_round_fetched - 2.
    /// Ignores errors from the context; failed lookups are simply not cached.
    pub async fn notify_round<C: ConsensusContext>(&self, round: Round, context: &mut C) {
        let (height, start) = {
            let d = self.data.read().unwrap();
            let last = d.last_round_fetched;
            if round <= last.saturating_sub(2) {
                return;
            }
            (d.height, last + 1)
        };
        let mut proposers = HashMap::new();
        let mut virtual_proposers = HashMap::new();
        for r in start..start + 5 {
            let round: Round = r;
            if let (Ok(proposer), Ok(virtual_proposer)) = (
                context.proposer(height, round).await,
                context.virtual_proposer(height, round).await,
            ) {
                proposers.insert(round, proposer);
                virtual_proposers.insert(round, virtual_proposer);
            }
        }
        if !proposers.is_empty() {
            let mut d = self.data.write().unwrap();
            d.proposers.extend(proposers);
            d.virtual_proposers.extend(virtual_proposers);
            d.last_round_fetched = start + 4;
        }
    }
}
