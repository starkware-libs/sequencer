use std::collections::HashSet;

use strum::{IntoEnumIterator, VariantNames};

use crate::errors::ExchangeRateOracleClientError;
use crate::{CurrencyPair, EthToFri, RateKind, StrkToUsd, LABEL_NAME_CURRENCY_PAIR};

/// Copy-pasted `pair_name` arms compile, so only a distinctness check catches them.
#[test]
fn pair_name_is_distinct_per_variant() {
    let pair_names: HashSet<&str> = CurrencyPair::iter().map(CurrencyPair::pair_name).collect();
    assert_eq!(pair_names.len(), CurrencyPair::VARIANTS.len());
}

#[test]
fn labels_yield_snake_case_variant_name() {
    for (currency_pair, snake_case_name) in [
        (CurrencyPair::EthUsd, "eth_usd"),
        (CurrencyPair::StrkUsd, "strk_usd"),
        (CurrencyPair::EthStrk, "eth_strk"),
    ] {
        assert_eq!(currency_pair.labels(), [(LABEL_NAME_CURRENCY_PAIR, snake_case_name)]);
    }
}

#[test]
fn enum_iter_and_variant_names_cover_all_variants() {
    assert_eq!(
        CurrencyPair::iter().collect::<Vec<CurrencyPair>>(),
        vec![CurrencyPair::EthUsd, CurrencyPair::StrkUsd, CurrencyPair::EthStrk]
    );
    assert_eq!(CurrencyPair::VARIANTS, ["eth_usd", "strk_usd", "eth_strk"]);
}

/// A transposed `RateKind::PAIR` compiles, since both markers map to a valid `CurrencyPair`.
#[test]
fn rate_kind_markers_map_to_their_pair() {
    assert_eq!(EthToFri::PAIR, CurrencyPair::EthStrk);
    assert_eq!(StrkToUsd::PAIR, CurrencyPair::StrkUsd);
}

/// Each message names the pair, the values that tripped the guard and the bound they violated.
#[test]
fn guard_errors_render_their_operator_facing_fields() {
    assert_eq!(
        ExchangeRateOracleClientError::StaleFeedError {
            pair_name: CurrencyPair::EthUsd.pair_name().to_string(),
            updated_at: 100,
            block_timestamp: 400,
            max_staleness_seconds: 120,
        }
        .to_string(),
        "Stale ETH/USD price feed: last updated at 100, priced for block timestamp 400, maximum \
         accepted staleness is 120 seconds"
    );
    assert_eq!(
        ExchangeRateOracleClientError::FutureFeedError {
            pair_name: CurrencyPair::StrkUsd.pair_name().to_string(),
            updated_at: 500,
            block_timestamp: 400,
            max_future_updated_at_seconds: 30,
        }
        .to_string(),
        "The STRK/USD price feed is dated 500, more than 30 seconds ahead of the block timestamp \
         400"
    );
    assert_eq!(
        ExchangeRateOracleClientError::RateOutOfBoundsError {
            pair_name: CurrencyPair::EthStrk.pair_name().to_string(),
            rate: 7,
            min_rate: 10,
            max_rate: 20,
        }
        .to_string(),
        "Rate 7 for ETH/STRK is outside the accepted range [10, 20]"
    );
    assert_eq!(
        ExchangeRateOracleClientError::ContractCallError(
            "retdata has 1 felt, expected 5".to_owned()
        )
        .to_string(),
        "Contract call to price feed failed: retdata has 1 felt, expected 5"
    );
    assert_eq!(
        ExchangeRateOracleClientError::ArithmeticError(
            "ETH/USD 3000 * 10^18 exceeds u128".to_owned()
        )
        .to_string(),
        "Arithmetic overflow while computing rate: ETH/USD 3000 * 10^18 exceeds u128"
    );
}
