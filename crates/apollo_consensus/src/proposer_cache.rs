//! Proposer cache to avoid blocking on proposer/virtual_proposer lookups.
//!
//! Prefetches proposers asynchronously; lookups are sync reads from the cache.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use starknet_api::block::BlockNumber;

use crate::types::{ConsensusContext, ConsensusError, Round, ValidatorId};

/// Number of rounds to prefetch ahead.
const PREFETCH_ROUNDS: u32 = 5;

/// Sync lookup for proposer and virtual proposer by round.
pub trait ProposerLookup: Send + Sync {
    fn actual_proposer(&self, round: Round) -> Result<ValidatorId, ConsensusError>;
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
pub(crate) struct ProposerCache {
    data: RwLock<CacheData>,
}

impl ProposerLookup for ProposerCache {
    fn actual_proposer(&self, round: Round) -> Result<ValidatorId, ConsensusError> {
        self.data
            .read()
            .expect("proposer cache read lock poisoned")
            .proposers
            .get(&round)
            .copied()
            .ok_or_else(|| {
                ConsensusError::InternalNetworkError(format!(
                    "Proposer not in cache for round {round}"
                ))
            })
    }

    fn virtual_proposer(&self, round: Round) -> Result<ValidatorId, ConsensusError> {
        self.data
            .read()
            .expect("proposer cache read lock poisoned")
            .virtual_proposers
            .get(&round)
            .copied()
            .ok_or_else(|| {
                ConsensusError::InternalNetworkError(format!(
                    "Virtual proposer not in cache for round {round}"
                ))
            })
    }
}

impl ProposerCache {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self { data: RwLock::new(CacheData::default()) })
    }

    /// Initialize for a height: set height, clear previous data, prefetch first 5 rounds.
    /// Call before creating SHC for each height.
    /// Ignores fetch errors; missing proposers will be handled at lookup time.
    pub(crate) async fn init<ContextT: ConsensusContext>(
        &self,
        height: BlockNumber,
        context: &mut ContextT,
    ) {
        self.clear_for_height(height);
        let (proposers, virtual_proposers) = Self::fetch_proposers(context, height, 0).await;
        self.extend_cache(proposers, virtual_proposers, PREFETCH_ROUNDS - 1);
    }

    /// Notify the cache that a round was seen and prefetch if needed.
    /// When round > last_round_fetched - 2, fetches [round, round + PREFETCH_ROUNDS) so the
    /// received round is in cache with PREFETCH_ROUNDS ahead.
    /// Ignores fetch errors; missing proposers will be handled at lookup time.
    pub(crate) async fn notify_round<ContextT: ConsensusContext>(
        &self,
        round: Round,
        context: &mut ContextT,
    ) {
        let (height, start) = {
            let d = self.data.read().expect("proposer cache read lock poisoned");
            let last = d.last_round_fetched;
            if round <= last.saturating_sub(2) {
                return;
            }
            (d.height, round)
        };
        let (proposers, virtual_proposers) = Self::fetch_proposers(context, height, start).await;
        self.extend_cache(proposers, virtual_proposers, start + PREFETCH_ROUNDS - 1);
    }

    fn clear_for_height(&self, height: BlockNumber) {
        let mut d = self.data.write().expect("proposer cache write lock poisoned");
        d.height = height;
        d.proposers.clear();
        d.virtual_proposers.clear();
    }

    fn extend_cache(
        &self,
        proposers: HashMap<Round, ValidatorId>,
        virtual_proposers: HashMap<Round, ValidatorId>,
        last_round_fetched: u32,
    ) {
        if proposers.is_empty() {
            return;
        }
        let mut d = self.data.write().expect("proposer cache write lock poisoned");
        d.proposers.extend(proposers);
        d.virtual_proposers.extend(virtual_proposers);
        d.last_round_fetched = last_round_fetched;
    }

    /// Fetches proposers and virtual proposers for rounds [start, start + PREFETCH_ROUNDS).
    /// Ignores failures; returns only successfully fetched entries.
    async fn fetch_proposers<ContextT: ConsensusContext>(
        context: &mut ContextT,
        height: BlockNumber,
        start: Round,
    ) -> (HashMap<Round, ValidatorId>, HashMap<Round, ValidatorId>) {
        let mut proposers = HashMap::new();
        let mut virtual_proposers = HashMap::new();
        for r in start..start + PREFETCH_ROUNDS {
            let round: Round = r;
            if let (Ok(proposer), Ok(virtual_proposer)) = (
                context.proposer(height, round).await,
                context.virtual_proposer(height, round).await,
            ) {
                proposers.insert(round, proposer);
                virtual_proposers.insert(round, virtual_proposer);
            }
        }
        (proposers, virtual_proposers)
    }
}
