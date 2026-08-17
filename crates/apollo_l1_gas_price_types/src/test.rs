use std::collections::HashSet;

use strum::{IntoEnumIterator, VariantNames};

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
