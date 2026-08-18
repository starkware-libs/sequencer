//! The Chainlink oracle client reading deployed mock aggregators through a real
//! `Batcher::call_contract`. The view call, the class execution and the retdata decoding are the
//! production ones, so the aggregator's storage is the only fixture, which is what the oracle's own
//! tests cannot cover: they mock the batcher the oracle reads through.

use apollo_batcher_types::batcher_types::{CallContractOutput, ProposeBlockInput};
use apollo_batcher_types::communication::{
    BatcherClient,
    BatcherClientError,
    BatcherClientResult,
    SharedBatcherClient,
};
use apollo_l1_gas_price::chainlink_oracle::{ChainlinkOracleClient, ChainlinkRate};
use apollo_l1_gas_price_config::config::{
    AllRateBoundsConfig,
    ChainlinkOracleConfig,
    FreshnessWindow,
};
use apollo_l1_gas_price_types::errors::ExchangeRateOracleClientError;
use apollo_l1_gas_price_types::{EthToFri, ExchangeRate, ExchangeRateOracleClientTrait, StrkToUsd};
use async_trait::async_trait;
use starknet_api::abi::abi_utils::get_storage_var_address;
use starknet_api::block::UnixTimestamp;
use starknet_api::contract_address;

use super::*;

const AGGREGATOR_CONTRACT: FeatureContract =
    FeatureContract::ChainlinkAggregatorMock(RunnableCairo1::Casm);

/// The block timestamp every read below is issued for, and every freshness bound is measured
/// against. Unrelated to the timestamp of the block the view call executes over, which the oracle
/// never reads.
const BLOCK_TIMESTAMP: u64 = 1_700_000_000;

/// The scale Chainlink's Starknet feeds report at today.
const FEED_DECIMALS: u8 = 8;
/// The lowest scale the oracle accepts a feed reporting.
const LOWEST_ACCEPTED_DECIMALS: u8 = 6;
/// Above the highest scale the oracle accepts a feed reporting, which is 18.
const OUT_OF_RANGE_DECIMALS: u8 = 20;

/// $0.15 per STRK at `FEED_DECIMALS`.
const STRK_USD_ANSWER: u128 = 15_000_000;
/// $0.15 per STRK at `LOWEST_ACCEPTED_DECIMALS`.
const STRK_USD_ANSWER_AT_LOWEST_ACCEPTED_DECIMALS: u128 = 150_000;
/// $3000 per ETH at `FEED_DECIMALS`.
const ETH_USD_ANSWER: u128 = 300_000_000_000;

/// $0.15 per STRK at the 18 decimals every rate the oracle returns carries.
const EXPECTED_STRK_TO_USD_RATE: ExchangeRate = 150_000_000_000_000_000;
/// $3000 / $0.15, that is 20,000 STRK per ETH, at the 18 decimals every rate carries.
const EXPECTED_ETH_TO_FRI_RATE: ExchangeRate = 20_000_000_000_000_000_000_000;

/// How long one `fetch_rate` may take to resolve past `QueryNotReadyError`, covering the real view
/// calls its background query makes.
const RATE_POLL_TIMEOUT: Duration = Duration::from_secs(60);
const RATE_POLL_INTERVAL: Duration = Duration::from_millis(50);

#[tokio::test(flavor = "multi_thread")]
async fn a_deployed_aggregator_reads_as_the_rescaled_strk_to_usd_rate() {
    let aggregators = deploy_aggregators(&[fresh_strk_usd_fixture()]).await;
    let feed_address = aggregators.feed_address(0);

    let rate = read_rate::<StrkToUsd>(
        aggregators.shared_client(),
        single_feed_config(feed_address),
        BLOCK_TIMESTAMP,
    )
    .await;

    assert_eq!(rate.unwrap(), EXPECTED_STRK_TO_USD_RATE);
}

/// The derived rate divides two independently read and independently rescaled USD legs.
#[tokio::test(flavor = "multi_thread")]
async fn two_deployed_aggregators_derive_the_eth_to_fri_rate() {
    let eth_usd_fixture = AggregatorFixture {
        answer: ETH_USD_ANSWER,
        updated_at: BLOCK_TIMESTAMP,
        feed_decimals: FEED_DECIMALS,
    };
    let aggregators = deploy_aggregators(&[eth_usd_fixture, fresh_strk_usd_fixture()]).await;

    let rate = read_rate::<EthToFri>(
        aggregators.shared_client(),
        chainlink_config(aggregators.feed_address(0), aggregators.feed_address(1)),
        BLOCK_TIMESTAMP,
    )
    .await;

    assert_eq!(rate.unwrap(), EXPECTED_ETH_TO_FRI_RATE);
}

/// One feed read is two view calls on the feed, `decimals` before the round, because a feed that
/// changed its scale must not be read at the previous one.
#[tokio::test(flavor = "multi_thread")]
async fn one_feed_read_calls_decimals_and_then_latest_round_data() {
    let aggregators = deploy_aggregators(&[fresh_strk_usd_fixture()]).await;
    let feed_address = aggregators.feed_address(0);

    read_rate::<StrkToUsd>(
        aggregators.shared_client(),
        single_feed_config(feed_address),
        BLOCK_TIMESTAMP,
    )
    .await
    .unwrap();

    assert_eq!(
        aggregators.view_calls(),
        vec![
            (feed_address, "decimals".to_string()),
            (feed_address, "latest_round_data".to_string()),
        ]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_round_exactly_at_the_staleness_bound_is_served() {
    let max_staleness_seconds = default_freshness_window().max_staleness_seconds;
    let aggregators =
        deploy_aggregators(&[strk_usd_fixture_dated_at(BLOCK_TIMESTAMP - max_staleness_seconds)])
            .await;

    let rate = read_rate::<StrkToUsd>(
        aggregators.shared_client(),
        single_feed_config(aggregators.feed_address(0)),
        BLOCK_TIMESTAMP,
    )
    .await;

    assert_eq!(rate.unwrap(), EXPECTED_STRK_TO_USD_RATE);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_round_one_second_past_the_staleness_bound_is_rejected() {
    let max_staleness_seconds = default_freshness_window().max_staleness_seconds;
    let aggregators = deploy_aggregators(&[strk_usd_fixture_dated_at(
        BLOCK_TIMESTAMP - max_staleness_seconds - 1,
    )])
    .await;

    let rate = read_rate::<StrkToUsd>(
        aggregators.shared_client(),
        single_feed_config(aggregators.feed_address(0)),
        BLOCK_TIMESTAMP,
    )
    .await;

    assert_matches!(rate, Err(ExchangeRateOracleClientError::StaleFeedError { updated_at, .. })
        if updated_at == BLOCK_TIMESTAMP - max_staleness_seconds - 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_round_exactly_at_the_future_bound_is_served() {
    let max_future_updated_at_seconds = default_freshness_window().max_future_updated_at_seconds;
    let aggregators = deploy_aggregators(&[strk_usd_fixture_dated_at(
        BLOCK_TIMESTAMP + max_future_updated_at_seconds,
    )])
    .await;

    let rate = read_rate::<StrkToUsd>(
        aggregators.shared_client(),
        single_feed_config(aggregators.feed_address(0)),
        BLOCK_TIMESTAMP,
    )
    .await;

    assert_eq!(rate.unwrap(), EXPECTED_STRK_TO_USD_RATE);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_round_one_second_past_the_future_bound_is_rejected() {
    let max_future_updated_at_seconds = default_freshness_window().max_future_updated_at_seconds;
    let aggregators = deploy_aggregators(&[strk_usd_fixture_dated_at(
        BLOCK_TIMESTAMP + max_future_updated_at_seconds + 1,
    )])
    .await;

    let rate = read_rate::<StrkToUsd>(
        aggregators.shared_client(),
        single_feed_config(aggregators.feed_address(0)),
        BLOCK_TIMESTAMP,
    )
    .await;

    assert_matches!(rate, Err(ExchangeRateOracleClientError::FutureFeedError { updated_at, .. })
        if updated_at == BLOCK_TIMESTAMP + max_future_updated_at_seconds + 1);
}

/// A feed reporting a scale outside the accepted range is rejected rather than rescaled by the
/// wrong power of ten.
#[tokio::test(flavor = "multi_thread")]
async fn a_feed_reporting_decimals_outside_the_accepted_range_is_rejected() {
    let out_of_range_fixture = AggregatorFixture {
        answer: STRK_USD_ANSWER,
        updated_at: BLOCK_TIMESTAMP,
        feed_decimals: OUT_OF_RANGE_DECIMALS,
    };
    let aggregators = deploy_aggregators(&[out_of_range_fixture]).await;

    let rate = read_rate::<StrkToUsd>(
        aggregators.shared_client(),
        single_feed_config(aggregators.feed_address(0)),
        BLOCK_TIMESTAMP,
    )
    .await;

    assert_matches!(
        rate,
        Err(ExchangeRateOracleClientError::InvalidRateError(message))
            if message.contains(&OUT_OF_RANGE_DECIMALS.to_string())
    );
}

/// A feed reporting a different, still accepted scale reaches the same rate as the 8-decimal feed
/// quoting the same price.
#[tokio::test(flavor = "multi_thread")]
async fn a_feed_reporting_the_lowest_accepted_decimals_rescales_to_the_same_rate() {
    let lowest_accepted_decimals_fixture = AggregatorFixture {
        answer: STRK_USD_ANSWER_AT_LOWEST_ACCEPTED_DECIMALS,
        updated_at: BLOCK_TIMESTAMP,
        feed_decimals: LOWEST_ACCEPTED_DECIMALS,
    };
    let aggregators = deploy_aggregators(&[lowest_accepted_decimals_fixture]).await;

    let rate = read_rate::<StrkToUsd>(
        aggregators.shared_client(),
        single_feed_config(aggregators.feed_address(0)),
        BLOCK_TIMESTAMP,
    )
    .await;

    assert_eq!(rate.unwrap(), EXPECTED_STRK_TO_USD_RATE);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_feed_answering_zero_is_rejected() {
    let aggregators = deploy_aggregators(&[AggregatorFixture {
        answer: 0,
        updated_at: BLOCK_TIMESTAMP,
        feed_decimals: FEED_DECIMALS,
    }])
    .await;

    let rate = read_rate::<StrkToUsd>(
        aggregators.shared_client(),
        single_feed_config(aggregators.feed_address(0)),
        BLOCK_TIMESTAMP,
    )
    .await;

    assert_matches!(
        rate,
        Err(ExchangeRateOracleClientError::InvalidRateError(message))
            if message.contains("zero answer")
    );
}

/// A feed address with no contract behind it fails the read on the batcher's own error, relayed
/// under the entry point that failed.
#[tokio::test(flavor = "multi_thread")]
async fn a_feed_address_with_no_deployed_contract_fails_the_read() {
    let aggregators = deploy_aggregators(&[fresh_strk_usd_fixture()]).await;
    let undeployed_address = contract_address!("0x1234567890abcdef1234567890abcdef");

    let rate = read_rate::<StrkToUsd>(
        aggregators.shared_client(),
        single_feed_config(undeployed_address),
        BLOCK_TIMESTAMP,
    )
    .await;

    assert_matches!(
        rate,
        Err(ExchangeRateOracleClientError::ContractCallError(message))
            if message.contains("decimals") && message.contains(&undeployed_address.to_string())
    );
}

/// The storage one deployed aggregator instance holds: the round it reports, and the scale it
/// reports that round at.
#[derive(Clone, Copy)]
struct AggregatorFixture {
    answer: u128,
    updated_at: u64,
    feed_decimals: u8,
}

/// Quotes `STRK_USD_ANSWER` at `FEED_DECIMALS`, dated exactly at `BLOCK_TIMESTAMP`.
fn fresh_strk_usd_fixture() -> AggregatorFixture {
    strk_usd_fixture_dated_at(BLOCK_TIMESTAMP)
}

fn strk_usd_fixture_dated_at(updated_at: u64) -> AggregatorFixture {
    AggregatorFixture { answer: STRK_USD_ANSWER, updated_at, feed_decimals: FEED_DECIMALS }
}

fn default_freshness_window() -> FreshnessWindow {
    ChainlinkOracleConfig::default().freshness
}

// A `StrkToUsd` client reads `strk_usd_feed_address` alone, so a test that exercises that client
// passes its single feed for both addresses rather than deploying an instance nothing reads.
fn single_feed_config(feed_address: ContractAddress) -> ChainlinkOracleConfig {
    chainlink_config(feed_address, feed_address)
}

fn chainlink_config(
    eth_usd_feed_address: ContractAddress,
    strk_usd_feed_address: ContractAddress,
) -> ChainlinkOracleConfig {
    ChainlinkOracleConfig {
        eth_usd_feed_address,
        strk_usd_feed_address,
        ..ChainlinkOracleConfig::default()
    }
}

/// A batcher over real storage holding one deployed aggregator instance per fixture it was built
/// from, and the addresses those instances live at.
struct DeployedAggregators {
    batcher_client: Arc<ViewCallRecordingBatcherClient>,
    feed_addresses: Vec<ContractAddress>,
}

impl DeployedAggregators {
    /// The address of the instance deployed for the fixture at `fixture_index`.
    fn feed_address(&self, fixture_index: usize) -> ContractAddress {
        self.feed_addresses[fixture_index]
    }

    fn shared_client(&self) -> SharedBatcherClient {
        self.batcher_client.clone()
    }

    fn view_calls(&self) -> Vec<(ContractAddress, String)> {
        self.batcher_client.view_calls()
    }
}

/// Deploys one aggregator instance per fixture, each seeded with that fixture's storage, in the
/// real storage of a batcher that serves view calls over it.
async fn deploy_aggregators(fixtures: &[AggregatorFixture]) -> DeployedAggregators {
    let num_instances =
        u16::try_from(fixtures.len()).expect("A test deploys far fewer instances than u16::MAX");
    let instance_ids: Vec<u16> = (0..num_instances).collect();
    let feed_addresses: Vec<ContractAddress> = instance_ids
        .iter()
        .map(|instance_id| AGGREGATOR_CONTRACT.get_instance_address(*instance_id))
        .collect();
    let storage_diffs = feed_addresses
        .iter()
        .copied()
        .zip(fixtures.iter().map(|fixture| aggregator_storage_diff(*fixture)))
        .collect();

    let mut mock_dependencies = deploy_contract_instances_in_real_storage(
        AGGREGATOR_CONTRACT,
        &instance_ids,
        storage_diffs,
    );
    let class_hash = AGGREGATOR_CONTRACT.get_class_hash();
    mock_dependencies
        .class_manager_client
        .expect_get_executable()
        .with(eq(class_hash))
        .returning(|_| Ok(Some(AGGREGATOR_CONTRACT.get_class())));
    mock_dependencies
        .class_manager_client
        .expect_get_sierra()
        .with(eq(class_hash))
        .returning(|_| Ok(Some(AGGREGATOR_CONTRACT.get_sierra())));

    DeployedAggregators {
        batcher_client: Arc::new(ViewCallRecordingBatcherClient::new(
            create_batcher_with_real_storage(mock_dependencies).await,
        )),
        feed_addresses,
    }
}

/// The mock aggregator's storage, in the `starknet::Store` layout its entry points read: `round`'s
/// fields occupy consecutive slots from the base slot of `round`, in declaration order, and
/// `feed_decimals` occupies the base slot of `feed_decimals`.
fn aggregator_storage_diff(fixture: AggregatorFixture) -> IndexMap<StorageKey, Felt> {
    // A phase-encoded `round_id`, `(phase_id << 128) | aggregator_round_id`, which exceeds every
    // primitive integer type and which the oracle consumes without decoding.
    const PHASE_ENCODED_ROUND_ID: &str = "0x100000000000000000000000000000042";
    const BLOCK_NUMBER: u64 = 987_654;
    // Held apart from `updated_at`, which the oracle judges freshness by, because `started_at` is
    // consumed without being decoded.
    const STARTED_AT: u64 = 1_699_999_000;

    let round_fields = [
        Felt::from_hex_unchecked(PHASE_ENCODED_ROUND_ID),
        Felt::from(fixture.answer),
        Felt::from(BLOCK_NUMBER),
        Felt::from(STARTED_AT),
        Felt::from(fixture.updated_at),
    ];
    let mut storage_diff = IndexMap::new();
    let mut field_key = get_storage_var_address("round", &[]);
    for field_value in round_fields {
        storage_diff.insert(field_key, field_value);
        field_key =
            field_key.next_storage_key().expect("A round field slot is within the key bound");
    }
    storage_diff
        .insert(get_storage_var_address("feed_decimals", &[]), Felt::from(fixture.feed_decimals));
    storage_diff
}

/// Polls `fetch_rate` past every `QueryNotReadyError` the client returns while its background query
/// is in flight, and returns the first outcome that query resolves to. The client is built once, so
/// a failure it holds is the failure this returns.
async fn read_rate<Kind: ChainlinkRate>(
    batcher_client: SharedBatcherClient,
    config: ChainlinkOracleConfig,
    block_timestamp: u64,
) -> Result<ExchangeRate, ExchangeRateOracleClientError> {
    let oracle_client =
        ChainlinkOracleClient::<Kind>::new(config, AllRateBoundsConfig::default(), batcher_client);
    tokio::time::timeout(RATE_POLL_TIMEOUT, async {
        loop {
            match oracle_client.fetch_rate(block_timestamp).await {
                Err(ExchangeRateOracleClientError::QueryNotReadyError(_)) => {
                    tokio::time::sleep(RATE_POLL_INTERVAL).await
                }
                resolved => return resolved,
            }
        }
    })
    .await
    .expect("The oracle's query over the deployed aggregators did not resolve in time")
}

const UNSERVED_REQUEST_REASON: &str =
    "The Chainlink oracle reaches the batcher through call_contract alone.";

/// A `BatcherClient` over a real `Batcher`, serving `call_contract` and recording every call it
/// serves.
struct ViewCallRecordingBatcherClient {
    batcher: Batcher,
    view_calls: Mutex<Vec<(ContractAddress, String)>>,
}

impl ViewCallRecordingBatcherClient {
    fn new(batcher: Batcher) -> Self {
        Self { batcher, view_calls: Mutex::new(Vec::new()) }
    }

    /// The contract address and entry point of every view call served so far, in the order they
    /// were served.
    fn view_calls(&self) -> Vec<(ContractAddress, String)> {
        self.view_calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl BatcherClient for ViewCallRecordingBatcherClient {
    async fn call_contract(
        &self,
        input: CallContractInput,
    ) -> BatcherClientResult<CallContractOutput> {
        self.view_calls.lock().unwrap().push((input.contract_address, input.entry_point.clone()));
        self.batcher.call_contract(input).await.map_err(BatcherClientError::BatcherError)
    }

    async fn propose_block(&self, _input: ProposeBlockInput) -> BatcherClientResult<()> {
        unimplemented!("{UNSERVED_REQUEST_REASON}")
    }

    async fn get_block_hash(&self, _block_number: BlockNumber) -> BatcherClientResult<BlockHash> {
        unimplemented!("{UNSERVED_REQUEST_REASON}")
    }

    async fn get_height(&self) -> BatcherClientResult<GetHeightResponse> {
        unimplemented!("{UNSERVED_REQUEST_REASON}")
    }

    async fn get_proposal_content(
        &self,
        _input: GetProposalContentInput,
    ) -> BatcherClientResult<GetProposalContentResponse> {
        unimplemented!("{UNSERVED_REQUEST_REASON}")
    }

    async fn validate_block(&self, _input: ValidateBlockInput) -> BatcherClientResult<()> {
        unimplemented!("{UNSERVED_REQUEST_REASON}")
    }

    async fn start_height(&self, _input: StartHeightInput) -> BatcherClientResult<()> {
        unimplemented!("{UNSERVED_REQUEST_REASON}")
    }

    async fn add_sync_block(&self, _sync_block: SyncBlock) -> BatcherClientResult<()> {
        unimplemented!("{UNSERVED_REQUEST_REASON}")
    }

    async fn decision_reached(
        &self,
        _input: DecisionReachedInput,
    ) -> BatcherClientResult<DecisionReachedResponse> {
        unimplemented!("{UNSERVED_REQUEST_REASON}")
    }

    async fn abort_proposal(&self, _proposal_id: ProposalId) -> BatcherClientResult<()> {
        unimplemented!("{UNSERVED_REQUEST_REASON}")
    }

    async fn finish_proposal(
        &self,
        _input: FinishProposalInput,
    ) -> BatcherClientResult<FinishProposalStatus> {
        unimplemented!("{UNSERVED_REQUEST_REASON}")
    }

    async fn send_txs_for_proposal(
        &self,
        _input: SendTxsForProposalInput,
    ) -> BatcherClientResult<SendTxsForProposalStatus> {
        unimplemented!("{UNSERVED_REQUEST_REASON}")
    }

    async fn revert_block(&self, _input: RevertBlockInput) -> BatcherClientResult<()> {
        unimplemented!("{UNSERVED_REQUEST_REASON}")
    }

    async fn get_batch_timestamp(&self) -> BatcherClientResult<UnixTimestamp> {
        unimplemented!("{UNSERVED_REQUEST_REASON}")
    }
}
