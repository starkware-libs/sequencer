use std::time::Duration;

use assert_matches::assert_matches;
use rstest::rstest;
use validator::Validate;

use super::{L1ProviderConfig, L1ScraperConfig};
use crate::config::L1MessageProviderConfig;
use crate::l1_scraper::L1_BLOCK_TIME;

#[rstest]
#[case::polling_interval(
    L1ScraperConfig {
        polling_interval_seconds: Duration::from_secs(2),
        finality: 0,
        ..L1ScraperConfig::default()
    }
)]
#[case::finality(
    L1ScraperConfig {
        polling_interval_seconds: Duration::from_secs(0),
        finality: 1,
        ..L1ScraperConfig::default()
    }
)]
#[case::polling_interval_and_finality(
    L1ScraperConfig {
        polling_interval_seconds: Duration::from_secs(1),
        finality: 1,
        ..L1ScraperConfig::default()
    }
)]
fn validate_l1_handler_cooldown_failure(#[case] l1_scraper_config: L1ScraperConfig) {
    let config = L1MessageProviderConfig {
        l1_scraper_config,
        l1_provider_config: L1ProviderConfig {
            new_l1_handler_cooldown_seconds: Duration::from_secs(1),
            ..L1ProviderConfig::default()
        },
    };

    assert_matches!(
        config.validate(),
        Err(e) if e.to_string().contains("L1 handler cooldown validation failed.")
    );
}

#[rstest]
#[case::polling_interval(
    L1ScraperConfig {
        polling_interval_seconds: Duration::from_secs(2),
        finality: 0,
        ..L1ScraperConfig::default()
    }
)]
#[case::finality(
    L1ScraperConfig {
        polling_interval_seconds: Duration::from_secs(0),
        finality: 1,
        ..L1ScraperConfig::default()
    }
)]
#[case::polling_interval_and_finality(
    L1ScraperConfig {
        polling_interval_seconds: Duration::from_secs(1),
        finality: 1,
        ..L1ScraperConfig::default()
    }
)]
fn validate_l1_handler_cooldown_success(#[case] l1_scraper_config: L1ScraperConfig) {
    let new_l1_handler_cooldown_seconds = l1_scraper_config.polling_interval_seconds
        + Duration::from_secs(L1_BLOCK_TIME * l1_scraper_config.finality)
        + Duration::from_secs(1);

    let config = L1MessageProviderConfig {
        l1_scraper_config,
        l1_provider_config: L1ProviderConfig {
            new_l1_handler_cooldown_seconds,
            ..L1ProviderConfig::default()
        },
    };

    assert!(config.validate().is_ok());
}
