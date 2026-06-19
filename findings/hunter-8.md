# Bug Findings: apollo_l1_gas_price

## Bug 1: Integer underflow panic in `fetch_rate` — timestamp smaller than lag_interval_seconds

**File**: `crates/apollo_l1_gas_price/src/exchange_rate_oracle.rs`, line 218

**Description**: `fetch_rate` unconditionally subtracts `lag_interval_seconds` from the caller-supplied `timestamp` before any bounds check:

```rust
let quantized_timestamp = (timestamp - self.config.lag_interval_seconds)
    .checked_div(self.config.lag_interval_seconds)
    .expect("lag_interval_seconds should be non-zero");
```

When `timestamp < lag_interval_seconds`, the subtraction is a u64 underflow. In debug builds this is a panic; in release builds (Rust wraps by default unless the `overflow-checks` profile flag is set) it silently wraps to a huge value, causing the oracle to query for a nonsensical future timestamp, likely returning an error from every URL and ultimately propagating `AllUrlsFailedError` to callers.

**Root Cause**: The subtraction `timestamp - self.config.lag_interval_seconds` uses plain `-` on `u64` without a prior bounds check or `saturating_sub`/`checked_sub`. A fresh chain (e.g., a local devnet), or any caller that passes a small timestamp, hits this immediately.

**Test**:

```rust
// In: crates/apollo_l1_gas_price/src/exchange_rate_oracle_test.rs
// (or add to the existing test file)

/// When the caller's timestamp is smaller than lag_interval_seconds, fetch_rate must not
/// panic and should return an error (not an astronomically large cached-price query).
#[tokio::test]
#[should_panic] // Bug: panics on underflow in debug builds.
async fn fetch_rate_panics_when_timestamp_less_than_lag() {
    use std::collections::BTreeMap;
    use apollo_config::converters::UrlAndHeaders;
    use url::Url;
    use apollo_l1_gas_price_types::ExchangeRateOracleClientTrait;
    use crate::exchange_rate_oracle::{ExchangeRateOracleClient, ExchangeRateOracleConfig};
    use crate::metrics::ETH_TO_STRK_ORACLE_METRICS;

    const LAG_INTERVAL_SECONDS: u64 = 3600; // 1 hour
    const TIMESTAMP: u64 = 60;              // 1 minute — clearly less than the lag

    let url_and_headers = UrlAndHeaders {
        url: Url::parse("http://127.0.0.1:9999/unused").unwrap(),
        headers: BTreeMap::new(),
    };
    let config = ExchangeRateOracleConfig {
        url_header_list: Some(vec![url_and_headers.into()]),
        lag_interval_seconds: LAG_INTERVAL_SECONDS,
        ..Default::default()
    };
    let client = ExchangeRateOracleClient::new(config, ETH_TO_STRK_ORACLE_METRICS);

    // This should NOT panic — it should return an error gracefully.
    // In debug builds it panics due to u64 underflow on `timestamp - lag_interval_seconds`.
    let _ = client.fetch_rate(TIMESTAMP).await;
}
```

**How to verify**:
```
cargo test -p apollo_l1_gas_price fetch_rate_panics_when_timestamp_less_than_lag
```
The test passes (is expected to panic) demonstrating the bug. The correct fix is to use `timestamp.checked_sub(self.config.lag_interval_seconds)` and return an error (e.g., `QueryNotReadyError`) if `None`.

---

## Bug 2: Integer underflow panic in `fetch_rate` — `quantized_timestamp` of zero causes `0 - 1` underflow

**File**: `crates/apollo_l1_gas_price/src/exchange_rate_oracle.rs`, line 237

**Description**: When the computed `quantized_timestamp` equals zero, the fallback lookup for the *previous* cached timestamp underflows:

```rust
const NUMBER_OF_TIMESTAMPS_BACK: u64 = 1;
// ...
if let Some(rate) = cache.get(&(quantized_timestamp - NUMBER_OF_TIMESTAMPS_BACK)) {
```

`quantized_timestamp` is zero whenever `timestamp` lies in `[lag_interval_seconds, 2 * lag_interval_seconds - 1]`. For example, with the default `lag_interval_seconds = 1` (from `ExchangeRateOracleConfig::default()`), this happens when `timestamp == 1`. In debug mode the arithmetic panics; in release mode it wraps to `u64::MAX`, causing a cache lookup for an astronomically large "previous" quantized timestamp (which is always a miss), returning `QueryNotReadyError` instead of the correct fallback.

**Root Cause**: The code attempts to look up `quantized_timestamp - 1` without guarding against `quantized_timestamp == 0`. The constant `NUMBER_OF_TIMESTAMPS_BACK = 1` is intentional but the zero-check is absent.

**Test**:

```rust
// In: crates/apollo_l1_gas_price/src/exchange_rate_oracle_test.rs

/// When the quantized timestamp equals zero (timestamp in [lag, 2*lag)), accessing the
/// "previous" cached timestamp must not underflow.
#[tokio::test]
#[should_panic] // Bug: 0u64 - 1 panics in debug builds.
async fn fetch_rate_panics_when_quantized_timestamp_is_zero() {
    use std::collections::BTreeMap;
    use apollo_config::converters::UrlAndHeaders;
    use url::Url;
    use apollo_l1_gas_price_types::ExchangeRateOracleClientTrait;
    use crate::exchange_rate_oracle::{ExchangeRateOracleClient, ExchangeRateOracleConfig};
    use crate::metrics::ETH_TO_STRK_ORACLE_METRICS;

    const LAG_INTERVAL_SECONDS: u64 = 60;
    // timestamp in [60, 119] gives quantized_timestamp = 0.
    // Picking 60 (the minimum safe value after Bug 1's subtraction).
    const TIMESTAMP: u64 = 60;

    let mut server = mockito::Server::new_async().await;
    // Set up a mock that will never respond fast enough — we want the query to stay in-flight
    // so that the code reaches the `quantized_timestamp - NUMBER_OF_TIMESTAMPS_BACK` branch.
    let _mock = server
        .mock("GET", mockito::Matcher::Any)
        .with_status(200)
        .with_header("Content-Type", "application/json")
        .with_body(serde_json::json!({"price": "0x1e240", "decimals": 18}).to_string())
        .with_delay(std::time::Duration::from_secs(30)) // simulate slow/no response
        .create();

    let url_and_headers = UrlAndHeaders {
        url: url::Url::parse(&server.url()).unwrap(),
        headers: BTreeMap::new(),
    };
    let config = ExchangeRateOracleConfig {
        url_header_list: Some(vec![url_and_headers.into()]),
        lag_interval_seconds: LAG_INTERVAL_SECONDS,
        query_timeout_sec: 60,
        ..Default::default()
    };
    let client = ExchangeRateOracleClient::new(config, ETH_TO_STRK_ORACLE_METRICS);

    // First call — spawns the background query, returns QueryNotReadyError.
    let _ = client.fetch_rate(TIMESTAMP).await;

    // Second call while the query is still in-flight: the code tries
    // `cache.get(&(quantized_timestamp - 1))` == `cache.get(&(0u64 - 1u64))`.
    // In debug mode this panics; in release mode it wraps silently.
    let _ = client.fetch_rate(TIMESTAMP).await; // panics here in debug
}
```

**How to verify**:
```
cargo test -p apollo_l1_gas_price fetch_rate_panics_when_quantized_timestamp_is_zero
```

The correct fix is to guard the subtraction:
```rust
if quantized_timestamp > 0 {
    if let Some(rate) = cache.get(&(quantized_timestamp - NUMBER_OF_TIMESTAMPS_BACK)) {
        return Ok(*rate);
    }
}
```

---

## Bug 3: Dead assertion in `gas_price_provider_adding_blocks` test — `matches!` without `assert!`

**File**: `crates/apollo_l1_gas_price/src/l1_gas_price_provider_test.rs`, line 100

**Description**: The test intends to assert that `get_price_info` returns `MissingDataError` after the ring buffer evicts the earliest block. The assertion is written as:

```rust
matches!(ret, Result::Err(L1GasPriceProviderError::MissingDataError { .. }));
```

`matches!` evaluates to a `bool` — the value is immediately discarded. The test always passes regardless of what `ret` actually contains, meaning the behaviour under test is completely unverified.

**Root Cause**: The author forgot to wrap the `matches!` call with `assert!`. This is a well-known Rust footgun; Clippy's `clippy::let_underscore_must_use` or `clippy::assert_matches` lints can catch it, but only if enabled.

Additionally, even with `assert!` the test would still not trigger the expected error because the ring buffer size defaults to `3000` blocks (from `..Default::default()`, which sets `storage_limit = 10 * 300`). Only 7 blocks are added, so no eviction ever occurs. To exercise the intended code path the config must set `storage_limit` to match `number_of_blocks_for_mean` (i.e., `3`).

**Test** (demonstrating both the dead assertion and the misconfigured ring buffer):

```rust
// In: crates/apollo_l1_gas_price/src/l1_gas_price_provider_test.rs

#[test]
fn gas_price_provider_adding_blocks_actually_evicts() {
    use std::sync::Arc;
    use apollo_l1_gas_price_config::config::L1GasPriceProviderConfig;
    use apollo_l1_gas_price_types::{GasPriceData, MockExchangeRateOracleClientTrait, PriceInfo};
    use starknet_api::block::{BlockTimestamp, GasPrice};
    use crate::l1_gas_price_provider::{L1GasPriceProvider, L1GasPriceProviderError};

    // Configure storage_limit == number_of_blocks_for_mean so the ring buffer is tight.
    const NUM_BLOCKS: usize = 3;
    let mut provider = L1GasPriceProvider::new(
        L1GasPriceProviderConfig {
            number_of_blocks_for_mean: NUM_BLOCKS as u64,
            storage_limit: NUM_BLOCKS, // ring buffer fits exactly 3 blocks
            ..Default::default()
        },
        Arc::new(MockExchangeRateOracleClientTrait::new()),
        Arc::new(MockExchangeRateOracleClientTrait::new()),
    );
    provider.initialize().unwrap();

    let lag = provider.config.lag_margin_seconds.as_secs();

    // Add exactly 3 blocks (fills the ring buffer).
    for i in 0u64..3 {
        provider
            .add_price_info(GasPriceData {
                block_number: i,
                timestamp: BlockTimestamp(i * 2),
                price_info: PriceInfo { base_fee_per_gas: GasPrice(i as u128), blob_fee: GasPrice(i as u128 + 1) },
            })
            .unwrap();
    }

    // Ask for prices at block 0's timestamp (with lag): this should succeed.
    let ts_block0_with_lag = 0u64 + lag;
    // (This may or may not succeed depending on lag; the key check is the next one.)

    // Add a 4th block — evicts block 0 from the ring buffer.
    provider
        .add_price_info(GasPriceData {
            block_number: 3,
            timestamp: BlockTimestamp(6),
            price_info: PriceInfo { base_fee_per_gas: GasPrice(10), blob_fee: GasPrice(11) },
        })
        .unwrap();

    // Now querying for a timestamp that requires block 0 (which was evicted) must fail.
    // The original test used `matches!` without `assert!` — this is the corrected version.
    let ret = provider.get_price_info(BlockTimestamp(ts_block0_with_lag));
    assert!(
        matches!(ret, Err(L1GasPriceProviderError::MissingDataError { .. })),
        "Expected MissingDataError after ring-buffer eviction, got: {ret:?}"
    );
}
```

**How to verify**:
```
cargo test -p apollo_l1_gas_price gas_price_provider_adding_blocks_actually_evicts
```

---

## Bug 4: u64 overflow in stale-price check — `*last_timestamp + max_time_gap_seconds`

**File**: `crates/apollo_l1_gas_price/src/l1_gas_price_provider.rs`, line 134

**Description**: The staleness guard computes:

```rust
if timestamp.0 > (*last_timestamp + self.config.max_time_gap_seconds) {
    return Err(L1GasPriceProviderError::StaleL1GasPricesError { ... });
}
```

Both operands are `u64`. If `*last_timestamp` is close to `u64::MAX` (e.g., caused by corrupted data, a test with synthetic timestamps, or a distant future chain), the addition wraps around to a small value in release builds, making `timestamp.0 >` evaluate to `false` — the stale-price guard is silently skipped. In debug builds the code panics.

**Root Cause**: Plain `+` on `u64` without `checked_add` or `saturating_add`. The fix is:

```rust
let stale_threshold = last_timestamp.saturating_add(self.config.max_time_gap_seconds);
if timestamp.0 > stale_threshold {
```

**Test**:

```rust
// In: crates/apollo_l1_gas_price/src/l1_gas_price_provider_test.rs

/// When last_timestamp is near u64::MAX and max_time_gap_seconds is non-zero, the addition
/// overflows in release mode and the staleness guard is bypassed.
#[test]
fn stale_check_bypassed_by_u64_overflow() {
    use std::sync::Arc;
    use apollo_l1_gas_price_config::config::L1GasPriceProviderConfig;
    use apollo_l1_gas_price_types::{GasPriceData, MockExchangeRateOracleClientTrait, PriceInfo};
    use starknet_api::block::{BlockTimestamp, GasPrice};
    use crate::l1_gas_price_provider::L1GasPriceProvider;

    // Construct a provider where max_time_gap_seconds is large enough to trigger overflow
    // when added to a near-MAX last_timestamp.
    let max_gap: u64 = 900;
    let last_ts: u64 = u64::MAX - (max_gap / 2); // addition will wrap past u64::MAX

    let mut provider = L1GasPriceProvider::new(
        L1GasPriceProviderConfig {
            number_of_blocks_for_mean: 1,
            storage_limit: 10,
            max_time_gap_seconds: max_gap,
            ..Default::default()
        },
        Arc::new(MockExchangeRateOracleClientTrait::new()),
        Arc::new(MockExchangeRateOracleClientTrait::new()),
    );
    provider.initialize().unwrap();

    provider
        .add_price_info(GasPriceData {
            block_number: 0,
            timestamp: BlockTimestamp(last_ts),
            price_info: PriceInfo { base_fee_per_gas: GasPrice(1), blob_fee: GasPrice(1) },
        })
        .unwrap();

    // The "current" timestamp is well past last_ts + max_gap, but the addition wraps.
    // In release mode the guard is silently bypassed; in debug mode it panics.
    // Either way the behaviour is wrong — the function should return StaleL1GasPricesError.
    let lag = provider.config.lag_margin_seconds.as_secs();
    // Use a timestamp slightly above last_ts so the lag subtraction finds the stored block,
    // yet far enough to be "stale" (if the guard worked correctly).
    // The wrapped threshold is a small number, so any timestamp > small_number "passes".
    let query_timestamp = last_ts.saturating_add(lag).saturating_add(1);
    let result = provider.get_price_info(BlockTimestamp(query_timestamp));

    // In release mode with the bug, this returns Ok(...) instead of Err(StaleL1GasPricesError).
    // The assertion below documents the expected (correct) behaviour.
    use crate::l1_gas_price_provider::L1GasPriceProviderError;
    assert!(
        matches!(result, Err(L1GasPriceProviderError::StaleL1GasPricesError { .. })),
        "Expected StaleL1GasPricesError but got: {result:?}"
    );
}
```

**How to verify**:
```
SEED=0 cargo test -p apollo_l1_gas_price stale_check_bypassed_by_u64_overflow
```
Run in release mode to see the silent bypass:
```
cargo test -p apollo_l1_gas_price --release stale_check_bypassed_by_u64_overflow
```

---

## Bug 5: `L1_GAS_PRICE_SCRAPER_LATEST_SCRAPED_BLOCK` metric is off by one — reports next-to-scrape, not last-scraped

**File**: `crates/apollo_l1_gas_price/src/l1_gas_price_scraper.rs`, lines 113 and 154

**Description**: In the `run` loop, the metric is updated *after* `update_prices` returns:

```rust
if let Err(e) = self.update_prices(&mut block_number).await { ... }
L1_GAS_PRICE_SCRAPER_LATEST_SCRAPED_BLOCK.set_lossy(block_number); // uses post-increment value
```

Inside `update_prices`, `*block_number` is incremented with `*block_number += 1` **after** each successful scrape. When the loop exits (either because `get_block_header` returned `None`, or because the `while` condition failed after processing all available blocks), `block_number` holds the **next** block to scrape, not the last block that was successfully scraped.

Concretely:
- If blocks 0–9 are scraped successfully and block 10 does not exist yet, `block_number` is `10` when `update_prices` returns. The metric records `10`, but the last scraped block is `9`.
- On error (e.g., `BaseLayerError` while fetching block 5 after scraping 0–4), `block_number` is `5`. The metric records `5`, labelling a **failed** block as "latest scraped".

**Root Cause**: The metric is positioned after the `block_number` variable has been advanced to the next-to-scrape position. The correct value to record is `block_number.saturating_sub(1)` (when at least one block was scraped), or the value should be tracked separately inside `update_prices`.

**Written justification** (no mechanical test because the metric API requires global metric registration in tests):

The invariant violation is clear from the code path: `*block_number += 1` runs at line 154 after `add_price_info` succeeds; `L1_GAS_PRICE_SCRAPER_LATEST_SCRAPED_BLOCK.set_lossy(block_number)` runs at line 113 after `update_prices` returns. There is no point in the control flow where `block_number` still equals the last successfully scraped block when the metric is written.

**Fix**: Change line 113 to:
```rust
// Record the last successfully scraped block, which is one before the next-to-scrape.
if block_number > 0 {
    L1_GAS_PRICE_SCRAPER_LATEST_SCRAPED_BLOCK.set_lossy(block_number - 1);
}
```
Or, better, track `last_scraped_block` as a separate variable updated inside `update_prices`.
