//! Config-level guard for serde round-trip symmetry.
//!
//! The fields covered here use `#[serde(deserialize_with = ..., serialize_with = ...)]` to read a
//! scalar/string wire shape (`Duration` as millis/seconds/float-seconds; comma-separated lists)
//! while the derived `Serialize` would otherwise emit a mismatched shape (`{secs, nanos}` or a JSON
//! array). `serde_json::to_value(config)` is used by the native-config harness and the startup-log
//! presentation, so the serializer MUST emit exactly the wire shape the deserializer reads.
//!
//! `apollo_config::converters_test` already proves each converter pairing round-trips on a wrapper
//! struct. This module is the complementary guard at the level of the REAL config structs: it would
//! catch a MISPAIRED `serialize_with` (e.g. a seconds field wired to the millis serializer) or a
//! future asymmetric field added WITHOUT a `serialize_with`. Each affected field is set to a
//! DISTINCTIVE non-default value, because a zero/`None`/default value can round-trip even when the
//! pairing is wrong.
//!
//! # Sensitive fields
//!
//! Some configs contain a `Sensitive<T>` field whose derived `Serialize` is intentionally
//! asymmetric (it redacts) and which is therefore left WITHOUT a `serialize_with` by design:
//!   - `apollo_network::NetworkConfig::secret_key: Option<Sensitive<Vec<u8>>>`
//!   - `apollo_l1_gas_price_config::ExchangeRateOracleConfig::url_header_list:
//!     Option<Vec<Sensitive<UrlAndHeaders>>>`
//!
//! Their deserializers read a string (empty string => `None`); the derived `Serialize` emits `null`
//! for a `None`, so a whole-struct `serde_json` round-trip fails on that one field even though
//! every field this commit touched is symmetric. To keep the guard on the non-`Sensitive` fields
//! without trying to make a `Sensitive` field round-trip, `assert_round_trips_ignoring_sensitive`
//! rewrites the serialized `Sensitive` entries to the empty string the deserializer maps back to
//! the `None` they hold here, then asserts the full struct round-trips.
//! `apollo_config::config_test`'s up-to-date guard covers the redacting `dump()` path for these
//! fields.

use std::time::Duration;

use apollo_consensus_config::config::{Timeout, TimeoutsConfig};
use apollo_consensus_manager_config::config::ConsensusManagerConfig;
use apollo_gateway_config::config::GatewayConfig;
use apollo_http_server_config::config::HttpServerConfig;
use apollo_l1_events_config::config::{L1EventsProviderConfig, L1EventsScraperConfig};
use apollo_l1_gas_price_config::config::{L1GasPriceProviderConfig, L1GasPriceScraperConfig};
use apollo_mempool_config::config::MempoolConfig;
use apollo_mempool_p2p_config::config::MempoolP2pConfig;
use apollo_network::NetworkConfig;
use apollo_state_sync_config::config::{CentralSyncClientConfig, StateSyncConfig};
use pretty_assertions::assert_eq;
use serde::Serialize;
use serde_json::Value;

/// Asserts that the derived `Serialize` emits exactly the wire shape the field deserializers read,
/// so `serde_json::from_value(serde_json::to_value(&value)) == value`.
fn assert_round_trips<T>(value: T)
where
    T: Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let json_value = serde_json::to_value(&value).unwrap();
    let round_tripped: T = serde_json::from_value(json_value).unwrap();
    assert_eq!(value, round_tripped);
}

/// Like [`assert_round_trips`], but first rewrites the given JSON pointers (each a `Sensitive`
/// field holding `None`, serialized by the derived `Serialize` as `null`) to the empty string its
/// deserializer maps back to `None`. This isolates the guard to the non-`Sensitive` fields, which
/// is where this commit's `serialize_with` attributes live; see the module docs.
fn assert_round_trips_ignoring_sensitive<T>(value: T, sensitive_field_pointers: &[&str])
where
    T: Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let mut json_value = serde_json::to_value(&value).unwrap();
    for pointer in sensitive_field_pointers {
        let entry = json_value
            .pointer_mut(pointer)
            .unwrap_or_else(|| panic!("sensitive field pointer {pointer} not found"));
        assert_eq!(*entry, Value::Null, "expected sensitive field {pointer} to be a None/null");
        *entry = Value::String(String::new());
    }
    let round_tripped: T = serde_json::from_value(json_value).unwrap();
    assert_eq!(value, round_tripped);
}

#[test]
fn consensus_manager_config_round_trips() {
    // Covers, in one nested config:
    //   - apollo_consensus_config: ConsensusDynamicConfig::sync_retry_interval (float-seconds),
    //     ConsensusStaticConfig::startup_delay (seconds), Timeout::{base, delta, max}
    //     (float-seconds via TimeoutsConfig, whose private fields are built through `Timeout::new`
    //     / `TimeoutsConfig::new`).
    //   - apollo_consensus_orchestrator_config: CendeConfig::{max_retry_duration_secs (seconds),
    //     min_retry_interval_ms, max_retry_interval_ms (millis)},
    //     ContextStaticConfig::{validate_proposal_margin_millis,
    //     retrospective_block_hash_retry_interval_millis} (millis).
    //   - apollo_network: NetworkConfig fields, via `set_network_config_non_default`.
    let mut config = ConsensusManagerConfig::default();

    let consensus = &mut config.consensus_manager_config;
    consensus.dynamic_config.sync_retry_interval = Duration::from_secs_f64(2.5);
    consensus.dynamic_config.timeouts = TimeoutsConfig::new(
        Timeout::new(
            Duration::from_secs_f64(1.5),
            Duration::from_secs_f64(0.25),
            Duration::from_secs_f64(8.0),
        ),
        Timeout::new(
            Duration::from_secs_f64(2.5),
            Duration::from_secs_f64(0.5),
            Duration::from_secs_f64(9.0),
        ),
        Timeout::new(
            Duration::from_secs_f64(3.5),
            Duration::from_secs_f64(0.75),
            Duration::from_secs_f64(10.0),
        ),
    );
    consensus.static_config.startup_delay = Duration::from_secs(7);

    let cende = &mut config.cende_config;
    cende.max_retry_duration_secs = Duration::from_secs(42);
    cende.min_retry_interval_ms = Duration::from_millis(123);
    cende.max_retry_interval_ms = Duration::from_millis(4567);

    let context_static = &mut config.context_config.static_config;
    context_static.validate_proposal_margin_millis = Duration::from_millis(789);
    context_static.retrospective_block_hash_retry_interval_millis = Duration::from_millis(321);

    set_network_config_non_default(&mut config.network_config);

    assert_round_trips_ignoring_sensitive(config, &["/network_config/secret_key"]);
}

#[test]
fn network_config_round_trips() {
    // Covers apollo_network directly: NetworkConfig::{session_timeout, idle_connection_timeout,
    // prune_dead_connections_ping_interval, prune_dead_connections_ping_timeout (seconds),
    // bootstrap_peer_multiaddr (comma-separated list)}, and (via sub-structs)
    // DiscoveryConfig::heartbeat_interval, RetryConfig::{max_delay_seconds,
    // new_connection_stabilization_millis}, PeerManagerConfig::{malicious_timeout_seconds,
    // unstable_timeout_millis}.
    let mut config = NetworkConfig::default();
    set_network_config_non_default(&mut config);
    assert_round_trips_ignoring_sensitive(config, &["/secret_key"]);
}

#[test]
fn mempool_p2p_config_round_trips() {
    // Covers apollo_mempool_p2p_config: MempoolP2pConfig::transaction_batch_rate_millis (millis),
    // and a second NetworkConfig instance (apollo_network).
    let mut config = MempoolP2pConfig::default();
    config.transaction_batch_rate_millis = Duration::from_millis(2468);
    set_network_config_non_default(&mut config.network_config);
    assert_round_trips_ignoring_sensitive(config, &["/network_config/secret_key"]);
}

#[test]
fn gateway_config_round_trips() {
    // Covers apollo_gateway_config: GatewayStaticConfig::authorized_declarer_accounts
    // (comma-separated list). Use a populated list so the comma-separated serializer is exercised;
    // a `None` would round-trip even with a mispaired serializer.
    use starknet_api::contract_address;

    let mut config = GatewayConfig::default();
    config.static_config.authorized_declarer_accounts =
        Some(vec![contract_address!("0x1"), contract_address!("0x2a")]);
    assert_round_trips(config);
}

#[test]
fn http_server_config_round_trips() {
    // Covers apollo_http_server_config: HttpServerStaticConfig::dynamic_config_poll_interval
    // (millis).
    let mut config = HttpServerConfig::default();
    config.static_config.dynamic_config_poll_interval = Duration::from_millis(1357);
    assert_round_trips(config);
}

#[test]
fn l1_events_provider_config_round_trips() {
    // Covers apollo_l1_events_config: L1EventsProviderConfig float-seconds fields.
    let config = L1EventsProviderConfig {
        startup_sync_sleep_retry_interval_seconds: Duration::from_secs_f64(1.5),
        l1_handler_cancellation_timelock_seconds: Duration::from_secs_f64(2.5),
        l1_handler_consumption_timelock_seconds: Duration::from_secs_f64(3.5),
        l1_handler_proposal_cooldown_seconds: Duration::from_secs_f64(4.5),
        ..Default::default()
    };
    assert_round_trips(config);
}

#[test]
fn l1_events_scraper_config_round_trips() {
    // Covers apollo_l1_events_config: L1EventsScraperConfig float-seconds fields.
    let mut config = L1EventsScraperConfig::default();
    config.startup_rewind_time_seconds = Duration::from_secs_f64(5.5);
    config.polling_interval_seconds = Duration::from_secs_f64(6.5);
    config.l1_block_time_seconds = Duration::from_secs_f64(12.0);
    assert_round_trips(config);
}

#[test]
fn l1_gas_price_provider_config_round_trips() {
    // Covers apollo_l1_gas_price_config: L1GasPriceProviderConfig::lag_margin_seconds
    // (float-seconds). The two nested ExchangeRateOracleConfigs each carry a `Sensitive`
    // `url_header_list` that this commit intentionally leaves asymmetric (its `Default` is a
    // populated, redacting list); set both to `None` so they serialize as null and ignore those two
    // fields. See the module docs.
    let mut config = L1GasPriceProviderConfig::default();
    config.lag_margin_seconds = Duration::from_secs_f64(33.5);
    config.eth_to_strk_oracle_config.url_header_list = None;
    config.strk_to_usd_oracle_config.url_header_list = None;
    assert_round_trips_ignoring_sensitive(
        config,
        &[
            "/eth_to_strk_oracle_config/url_header_list",
            "/strk_to_usd_oracle_config/url_header_list",
        ],
    );
}

#[test]
fn l1_gas_price_scraper_config_round_trips() {
    // Covers apollo_l1_gas_price_config: L1GasPriceScraperConfig::polling_interval (float-seconds).
    let mut config = L1GasPriceScraperConfig::default();
    config.polling_interval = Duration::from_secs_f64(7.5);
    assert_round_trips(config);
}

#[test]
fn mempool_config_round_trips() {
    // Covers apollo_mempool_config: MempoolDynamicConfig::transaction_ttl (seconds) and
    // MempoolStaticConfig::declare_delay (seconds).
    let mut config = MempoolConfig::default();
    config.dynamic_config.transaction_ttl = Duration::from_secs(111);
    config.static_config.declare_delay = Duration::from_secs(13);
    assert_round_trips(config);
}

#[test]
fn state_sync_config_round_trips() {
    // Reaches two crates not directly imported here, through public fields of StateSyncConfig:
    //   - apollo_p2p_sync_config: P2pSyncClientConfig::{wait_period_for_new_data,
    //     wait_period_for_other_protocol} (millis).
    //   - apollo_central_sync_config: SyncConfig::{latest_block_poll_interval_millis (millis),
    //     base_layer_propagation_sleep_duration, recoverable_error_sleep_duration (seconds)},
    //     reached via CentralSyncClientConfig::sync_config.
    //
    // Validation (exactly one of p2p/central must be set) is NOT run by a serde round-trip, so both
    // are populated here to exercise both crates' fields in a single round-trip.
    // Two Sensitive fields are reached and ignored (see the module docs):
    // CentralSourceConfig::http_headers (`Option<Sensitive<HashMap<..>>>`) and the `secret_key` of
    // StateSyncStaticConfig::network_config (default `Some(NetworkConfig { .. })`); both are None
    // here, serialized as null.
    let mut config = StateSyncConfig::default();

    let p2p_sync = config.static_config.p2p_sync_client_config.get_or_insert_with(Default::default);
    p2p_sync.wait_period_for_new_data = Duration::from_millis(1234);
    p2p_sync.wait_period_for_other_protocol = Duration::from_millis(5678);

    let central_sync = config
        .static_config
        .central_sync_client_config
        .get_or_insert_with(CentralSyncClientConfig::default);
    central_sync.sync_config.latest_block_poll_interval_millis = Duration::from_millis(987);
    central_sync.sync_config.base_layer_propagation_sleep_duration = Duration::from_secs(17);
    central_sync.sync_config.recoverable_error_sleep_duration = Duration::from_secs(23);

    if let Some(network_config) = config.static_config.network_config.as_mut() {
        set_network_config_non_default(network_config);
    }

    assert_round_trips_ignoring_sensitive(
        config,
        &[
            "/static_config/central_sync_client_config/central_source_config/http_headers",
            "/static_config/network_config/secret_key",
        ],
    );
}

// papyrus_base_layer::EthereumBaseLayerConfig is intentionally NOT round-tripped here. Its
// `ordered_l1_endpoint_urls: Vec<Sensitive<Url>>` field uses `deserialize_vec` (which reads a
// single space-separated String) while the derived Serialize emits a JSON array of redacted
// strings, so the struct cannot round-trip through `serde_json` regardless of the value -- this
// asymmetry is by design (the `Sensitive` serializer redacts), and unlike
// `secret_key`/`url_header_list` it has no empty-string `None` form to substitute. Its two duration
// fields (`timeout_millis`, `retry_primary_interval_seconds`) gained `serialize_with` in this
// commit; their converter pairings (`deserialize_milliseconds_to_duration`/
// `serialize_duration_as_milliseconds` and `deserialize_seconds_to_duration`/
// `serialize_duration_as_seconds`) are exercised by the wrapper round-trip tests in
// `apollo_config::converters_test`.

/// Sets every `serialize_with`-affected field on `NetworkConfig` (and its nested `DiscoveryConfig`
/// and `RetryConfig`) to a distinctive non-default value. `PeerManagerConfig`'s two affected fields
/// are private; they keep their non-zero, distinct defaults (1s vs 1000ms), which still differ
/// between the seconds and millis serializers, so a mispairing there is caught. `secret_key` is
/// left `None` (its `Sensitive` serializer redacts and cannot round-trip; see the module docs).
fn set_network_config_non_default(network_config: &mut NetworkConfig) {
    network_config.session_timeout = Duration::from_secs(37);
    network_config.idle_connection_timeout = Duration::from_secs(41);
    network_config.prune_dead_connections_ping_interval = Duration::from_secs(19);
    network_config.prune_dead_connections_ping_timeout = Duration::from_secs(29);
    // A multiaddr whose `to_string()` round-trips through `Multiaddr::from_str`. No
    // `/p2p/<peer-id>` component (validation, which requires it, is not run by a serde
    // round-trip).
    network_config.bootstrap_peer_multiaddr = Some(vec!["/ip4/1.2.3.4/tcp/10000".parse().unwrap()]);

    network_config.discovery_config.heartbeat_interval = Duration::from_millis(875);
    network_config.discovery_config.bootstrap_dial_retry_config.max_delay_seconds =
        Duration::from_secs(53);
    network_config
        .discovery_config
        .bootstrap_dial_retry_config
        .new_connection_stabilization_millis = Duration::from_millis(631);
}
