use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use apollo_config::secrets::Sensitive;
use apollo_infra_utils::url::to_safe_string;
use metrics_exporter_prometheus::PrometheusBuilder;
use rstest::rstest;
use starknet_api::block::BlockHashAndNumber;
use url::Url;

use crate::cyclic_base_layer_wrapper::CyclicBaseLayerWrapper;
use crate::metrics::{
    ScraperLabel,
    L1_PRIMARY_ENDPOINT_DOWN_SINCE_TIMESTAMP_SECONDS,
    LABEL_NAME_SCRAPER,
};
use crate::{BaseLayerContract, L1BlockHeader, L1BlockReference, MockBaseLayerContract, MockError};

fn get_url_helper(num_url_calls_made: &AtomicUsize) -> Result<Sensitive<Url>, MockError> {
    if num_url_calls_made.load(Ordering::Relaxed).is_multiple_of(2) {
        Ok(Sensitive::new(Url::parse("http://first_endpoint").unwrap())
            .with_redactor(to_safe_string))
    } else {
        Ok(Sensitive::new(Url::parse("http://second_endpoint").unwrap())
            .with_redactor(to_safe_string))
    }
}

// In the following tests, we specify the number of failed calls, each to a different URL endpoint.
// Once the number of failures are reached, we expect to get a success or a failure, depending on
// whether there are any URLs left to try. We use a base layer with two URLs that get cycled.

const NUM_URLS: usize = 2;
const TEST_RETRY_PRIMARY_INTERVAL: Duration = Duration::from_secs(3600);

#[rstest]
#[case::success(0)]
#[case::fail_first(1)]
#[case::fail_both(2)]
#[tokio::test]
async fn cycle_get_proved_block_at(#[case] num_failing_calls: usize) {
    // Setup.
    let success = num_failing_calls < NUM_URLS;
    let expected_num_url_calls = num_failing_calls * 2 + 1;
    let num_url_calls_made = Arc::new(AtomicUsize::new(0));
    let num_url_calls_made_clone = num_url_calls_made.clone();
    let num_url_calls_made_clone2 = num_url_calls_made.clone();

    let mut base_layer = MockBaseLayerContract::new();
    base_layer.expect_is_at_primary().returning(|| Ok(false));
    base_layer
        .expect_get_proved_block_at()
        .times(num_failing_calls)
        .returning(move |_| Err(MockError::MockError));
    base_layer
        .expect_get_proved_block_at()
        .times(usize::from(success))
        .returning(move |_| Ok(BlockHashAndNumber::default()));
    base_layer
        .expect_get_url()
        .times(expected_num_url_calls)
        .returning(move || get_url_helper(&num_url_calls_made_clone));
    base_layer.expect_cycle_provider_url().times(num_failing_calls).returning(move || {
        num_url_calls_made_clone2.fetch_add(1, Ordering::Relaxed);
        Ok(())
    });
    let mut wrapper = CyclicBaseLayerWrapper::new(
        base_layer,
        TEST_RETRY_PRIMARY_INTERVAL,
        ScraperLabel::L1Events,
    );

    // Test.
    let result = wrapper.get_proved_block_at(1).await;

    // Check we got success/failure.
    if success {
        assert!(result.is_ok());
    } else {
        assert!(result.is_err());
    }
}

#[rstest]
#[case::success(0)]
#[case::fail_first(1)]
#[case::fail_both(2)]
#[tokio::test]
async fn cycle_latest_l1_block_number(#[case] num_failing_calls: usize) {
    // Setup.
    let success = num_failing_calls < NUM_URLS;
    let expected_num_url_calls = num_failing_calls * 2 + 1;
    let num_url_calls_made = Arc::new(AtomicUsize::new(0));
    let num_url_calls_made_clone = num_url_calls_made.clone();
    let num_url_calls_made_clone2 = num_url_calls_made.clone();

    let mut base_layer = MockBaseLayerContract::new();
    base_layer.expect_is_at_primary().returning(|| Ok(false));
    base_layer
        .expect_latest_l1_block_number()
        .times(num_failing_calls)
        .returning(move || Err(MockError::MockError));
    base_layer.expect_latest_l1_block_number().times(usize::from(success)).returning(move || Ok(1));
    base_layer
        .expect_get_url()
        .times(expected_num_url_calls)
        .returning(move || get_url_helper(&num_url_calls_made_clone));
    base_layer.expect_cycle_provider_url().times(num_failing_calls).returning(move || {
        num_url_calls_made_clone2.fetch_add(1, Ordering::Relaxed);
        Ok(())
    });
    let mut wrapper = CyclicBaseLayerWrapper::new(
        base_layer,
        TEST_RETRY_PRIMARY_INTERVAL,
        ScraperLabel::L1Events,
    );

    // Test.
    let result = wrapper.latest_l1_block_number().await;

    // Check we got success/failure.
    if success {
        assert!(result.is_ok());
    } else {
        assert!(result.is_err());
    }
}

#[rstest]
#[case::success(0)]
#[case::fail_first(1)]
#[case::fail_both(2)]
#[tokio::test]
async fn cycle_l1_block_at(#[case] num_failing_calls: usize) {
    // Setup.
    let success = num_failing_calls < NUM_URLS;
    let expected_num_url_calls = num_failing_calls * 2 + 1;
    let num_url_calls_made = Arc::new(AtomicUsize::new(0));
    let num_url_calls_made_clone = num_url_calls_made.clone();
    let num_url_calls_made_clone2 = num_url_calls_made.clone();

    let mut base_layer = MockBaseLayerContract::new();
    base_layer.expect_is_at_primary().returning(|| Ok(false));
    base_layer
        .expect_l1_block_at()
        .times(num_failing_calls)
        .returning(move |_| Err(MockError::MockError));
    base_layer
        .expect_l1_block_at()
        .times(usize::from(success))
        .returning(move |_| Ok(Some(L1BlockReference::default())));
    base_layer
        .expect_get_url()
        .times(expected_num_url_calls)
        .returning(move || get_url_helper(&num_url_calls_made_clone));
    base_layer.expect_cycle_provider_url().times(num_failing_calls).returning(move || {
        num_url_calls_made_clone2.fetch_add(1, Ordering::Relaxed);
        Ok(())
    });
    let mut wrapper = CyclicBaseLayerWrapper::new(
        base_layer,
        TEST_RETRY_PRIMARY_INTERVAL,
        ScraperLabel::L1Events,
    );

    // Test.
    let result = wrapper.l1_block_at(1).await;

    // Check we got success/failure.
    if success {
        assert!(result.is_ok());
    } else {
        assert!(result.is_err());
    }
}

#[rstest]
#[case::success(0)]
#[case::fail_first(1)]
#[case::fail_both(2)]
#[tokio::test]
async fn cycle_events(#[case] num_failing_calls: usize) {
    // Setup.
    let success = num_failing_calls < NUM_URLS;
    let expected_num_url_calls = num_failing_calls * 2 + 1;
    let num_url_calls_made = Arc::new(AtomicUsize::new(0));
    let num_url_calls_made_clone = num_url_calls_made.clone();
    let num_url_calls_made_clone2 = num_url_calls_made.clone();

    let mut base_layer = MockBaseLayerContract::new();
    base_layer.expect_is_at_primary().returning(|| Ok(false));
    base_layer
        .expect_events()
        .times(num_failing_calls)
        .returning(move |_, _| Err(MockError::MockError));
    base_layer.expect_events().times(usize::from(success)).returning(move |_, _| Ok(vec![]));
    base_layer
        .expect_get_url()
        .times(expected_num_url_calls)
        .returning(move || get_url_helper(&num_url_calls_made_clone));
    base_layer.expect_cycle_provider_url().times(num_failing_calls).returning(move || {
        num_url_calls_made_clone2.fetch_add(1, Ordering::Relaxed);
        Ok(())
    });
    let mut wrapper = CyclicBaseLayerWrapper::new(
        base_layer,
        TEST_RETRY_PRIMARY_INTERVAL,
        ScraperLabel::L1Events,
    );

    // Test.
    let result = wrapper.events(0..=1_u64, &[]).await;

    // Check we got success/failure.
    if success {
        assert!(result.is_ok());
    } else {
        assert!(result.is_err());
    }
}

#[rstest]
#[case::success(0)]
#[case::fail_first(1)]
#[case::fail_both(2)]
#[tokio::test]
async fn cycle_get_block_header(#[case] num_failing_calls: usize) {
    // Setup.
    let success = num_failing_calls < NUM_URLS;
    let expected_num_url_calls = num_failing_calls * 2 + 1;
    let num_url_calls_made = Arc::new(AtomicUsize::new(0));
    let num_url_calls_made_clone = num_url_calls_made.clone();
    let num_url_calls_made_clone2 = num_url_calls_made.clone();

    let mut base_layer = MockBaseLayerContract::new();
    base_layer.expect_is_at_primary().returning(|| Ok(false));
    base_layer
        .expect_get_block_header()
        .times(num_failing_calls)
        .returning(move |_| Err(MockError::MockError));
    base_layer
        .expect_get_block_header()
        .times(usize::from(success))
        .returning(move |_| Ok(Some(L1BlockHeader::default())));
    base_layer
        .expect_get_url()
        .times(expected_num_url_calls)
        .returning(move || get_url_helper(&num_url_calls_made_clone));
    base_layer.expect_cycle_provider_url().times(num_failing_calls).returning(move || {
        num_url_calls_made_clone2.fetch_add(1, Ordering::Relaxed);
        Ok(())
    });
    let mut wrapper = CyclicBaseLayerWrapper::new(
        base_layer,
        TEST_RETRY_PRIMARY_INTERVAL,
        ScraperLabel::L1Events,
    );

    // Test.
    let result = wrapper.get_block_header(1).await;

    // Check we got success/failure.
    if success {
        assert!(result.is_ok());
    } else {
        assert!(result.is_err());
    }
}

// In this test we try to get a block header (it could be any other call), but before we start we
// get a failure to get the URL.
#[tokio::test]
async fn get_url_itself_fails() {
    // Setup.
    let mut base_layer = MockBaseLayerContract::new();

    base_layer.expect_is_at_primary().returning(|| Ok(false));
    base_layer.expect_get_url().times(1).returning(move || Err(MockError::MockError));
    let mut wrapper = CyclicBaseLayerWrapper::new(
        base_layer,
        TEST_RETRY_PRIMARY_INTERVAL,
        ScraperLabel::L1Events,
    );

    // Test.
    let result = wrapper.get_block_header(1).await;
    assert!(result.is_err());
    assert!(matches!(result, Err(MockError::MockError)));
}

// In this test we try to get a block header (it could be any other call), but after getting a
// failure, we try to cycle the URLs and get an error on the cycle itself.
#[tokio::test]
async fn get_block_header_fails_after_cycle_error() {
    // Setup.
    let mut base_layer = MockBaseLayerContract::new();
    base_layer.expect_is_at_primary().returning(|| Ok(false));
    base_layer.expect_get_url().times(2).returning(move || {
        Ok(Sensitive::new(Url::parse("http://first_endpoint").unwrap())
            .with_redactor(to_safe_string))
    });
    base_layer.expect_get_block_header().times(1).returning(move |_| Err(MockError::MockError));
    base_layer.expect_cycle_provider_url().times(1).returning(move || Err(MockError::MockError));
    let mut wrapper = CyclicBaseLayerWrapper::new(
        base_layer,
        TEST_RETRY_PRIMARY_INTERVAL,
        ScraperLabel::L1Events,
    );

    // Test.
    let result = wrapper.get_block_header(1).await;
    assert!(result.is_err());
    assert!(matches!(result, Err(MockError::MockError)));
}

// This function doesn't cycle the provider URL. It either fails or succeeds on the first call.
#[rstest]
#[case::success(true)]
#[case::fail(false)]
#[tokio::test]
async fn pass_through_get_block_header_immutable(#[case] success: bool) {
    // Setup.
    let mut base_layer = MockBaseLayerContract::new();
    if success {
        base_layer
            .expect_get_block_header_immutable()
            .returning(move |_| Ok(Some(L1BlockHeader::default())));
    } else {
        base_layer
            .expect_get_block_header_immutable()
            .returning(move |_| Err(MockError::MockError));
    }
    let wrapper = CyclicBaseLayerWrapper::new(
        base_layer,
        TEST_RETRY_PRIMARY_INTERVAL,
        ScraperLabel::L1Events,
    );

    // Test.
    let result = wrapper.get_block_header_immutable(1).await;

    // Check we got success/failure.
    if success {
        assert!(result.is_ok());
    } else {
        assert!(result.is_err());
    }
}

#[tokio::test]
async fn pass_through_get_url() {
    // Setup.
    let mut base_layer = MockBaseLayerContract::new();
    base_layer.expect_get_url().returning(move || {
        Ok(Sensitive::new(Url::parse("http://first_endpoint").unwrap())
            .with_redactor(to_safe_string))
    });
    let wrapper = CyclicBaseLayerWrapper::new(
        base_layer,
        TEST_RETRY_PRIMARY_INTERVAL,
        ScraperLabel::L1Events,
    );

    // Test.
    let result = wrapper.get_url().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn pass_through_set_provider_url() {
    // Setup.
    let mut base_layer = MockBaseLayerContract::new();
    base_layer.expect_set_provider_url().returning(move |_| Ok(()));
    let mut wrapper = CyclicBaseLayerWrapper::new(
        base_layer,
        TEST_RETRY_PRIMARY_INTERVAL,
        ScraperLabel::L1Events,
    );

    // Test.
    let result = wrapper
        .set_provider_url(
            Sensitive::new(Url::parse("http://first_endpoint").unwrap())
                .with_redactor(to_safe_string),
        )
        .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn pass_through_cycle_provider_url() {
    // Setup.
    let mut base_layer = MockBaseLayerContract::new();
    base_layer.expect_cycle_provider_url().returning(move || Ok(()));
    let mut wrapper = CyclicBaseLayerWrapper::new(
        base_layer,
        TEST_RETRY_PRIMARY_INTERVAL,
        ScraperLabel::L1Events,
    );

    // Test.
    let result = wrapper.cycle_provider_url().await;
    assert!(result.is_ok());
}

// Verifies that when an operation begins on a non-primary endpoint and every attempt fails, the
// wrapper cycles through the entire URL list exactly once and returns the last error. The operation
// starts on the tertiary endpoint, and cycling advances tertiary -> primary -> secondary ->
// tertiary, at which point the wrapper detects it wrapped back to the start URL and returns the
// error.
#[tokio::test]
async fn test_exhaust_from_non_primary_index_returns_last_error() {
    // Setup: model a 3-URL list where the operation starts on the tertiary (non-primary) endpoint.
    let primary_url = Sensitive::new(Url::parse("http://primary_endpoint").unwrap())
        .with_redactor(to_safe_string);
    let secondary_url = Sensitive::new(Url::parse("http://secondary_endpoint").unwrap())
        .with_redactor(to_safe_string);
    let tertiary_url = Sensitive::new(Url::parse("http://tertiary_endpoint").unwrap())
        .with_redactor(to_safe_string);

    // The wrapper calls get_url once for start_url, then per failed attempt calls get_url (current)
    // + cycle_provider_url + get_url (new). With a tertiary start and all three attempts failing,
    // the get_url return sequence is: start=tertiary; then current/new pairs as the index advances
    // tertiary -> primary -> secondary -> tertiary.
    let url_sequence = [
        tertiary_url.clone(),
        tertiary_url.clone(),
        primary_url.clone(),
        primary_url,
        secondary_url.clone(),
        secondary_url,
        tertiary_url,
    ];
    const NUM_ATTEMPTS: usize = 3;
    let num_get_url_calls = url_sequence.len();
    // Each mocked call pops the next queued return, so the return order is explicit per call.
    let mut get_url_returns = url_sequence.into_iter();
    let mut error_returns = (0..NUM_ATTEMPTS).map(MockError::Numbered);

    let mut base_layer = MockBaseLayerContract::new();
    base_layer.expect_is_at_primary().returning(|| Ok(false));
    base_layer
        .expect_get_url()
        .times(num_get_url_calls)
        .returning(move || Ok(get_url_returns.next().unwrap()));
    // Every attempt fails with a distinct numbered error so we can prove the last one is returned.
    base_layer
        .expect_get_proved_block_at()
        .times(NUM_ATTEMPTS)
        .returning(move |_| Err(error_returns.next().unwrap()));
    // Cycling succeeds once per failed attempt.
    base_layer.expect_cycle_provider_url().times(NUM_ATTEMPTS).returning(|| Ok(()));
    // Use a large retry-primary interval so the primary retry never interferes with the cycling
    // path.
    let mut wrapper = CyclicBaseLayerWrapper::new(
        base_layer,
        TEST_RETRY_PRIMARY_INTERVAL,
        ScraperLabel::L1Events,
    );

    // Test: all attempts fail; the wrapper must return the last attempt's error.
    let result = wrapper.get_proved_block_at(1).await;
    assert_eq!(result, Err(MockError::Numbered(NUM_ATTEMPTS - 1)));
}

// Verifies that when the retry-primary interval has elapsed, the wrapper calls
// reset_provider_url_to_primary before executing the cycling operation.
#[tokio::test]
async fn test_retry_primary_when_interval_elapsed() {
    // Setup: use Duration::ZERO so the interval is always elapsed.
    let mut base_layer = MockBaseLayerContract::new();
    base_layer.expect_is_at_primary().returning(|| Ok(false));
    base_layer.expect_reset_provider_url_to_primary().times(1).returning(|| Ok(()));
    base_layer.expect_get_url().times(1).returning(|| {
        Ok(Sensitive::new(Url::parse("http://first_endpoint").unwrap())
            .with_redactor(to_safe_string))
    });
    base_layer
        .expect_get_proved_block_at()
        .times(1)
        .returning(|_| Ok(BlockHashAndNumber::default()));
    let mut wrapper =
        CyclicBaseLayerWrapper::new(base_layer, Duration::ZERO, ScraperLabel::L1Events);

    // Test.
    let result = wrapper.get_proved_block_at(1).await;
    assert!(result.is_ok());
}

// Verifies that when the retry-primary interval has elapsed but reset_provider_url_to_primary
// returns an error, the error propagates and the underlying operation is never called.
#[tokio::test]
async fn test_retry_primary_error_propagates_and_skips_operation() {
    // Setup: use Duration::ZERO so the interval is always elapsed.
    let mut base_layer = MockBaseLayerContract::new();
    base_layer.expect_is_at_primary().returning(|| Ok(false));
    base_layer
        .expect_reset_provider_url_to_primary()
        .times(1)
        .returning(|| Err(MockError::MockError));
    // The underlying operation must never be reached when retry-primary errors out.
    base_layer.expect_get_proved_block_at().times(0);

    let mut wrapper =
        CyclicBaseLayerWrapper::new(base_layer, Duration::ZERO, ScraperLabel::L1Events);

    // Test.
    let result = wrapper.get_proved_block_at(1).await;
    assert!(matches!(result, Err(MockError::MockError)));
}

// Verifies that when the retry-primary interval has not elapsed, the wrapper does not call
// reset_provider_url_to_primary.
#[tokio::test]
async fn test_no_retry_primary_when_interval_not_elapsed() {
    // Setup: use a large interval so it is never elapsed.
    let mut base_layer = MockBaseLayerContract::new();
    base_layer.expect_is_at_primary().returning(|| Ok(false));
    base_layer.expect_reset_provider_url_to_primary().times(0);
    base_layer.expect_get_url().times(1).returning(|| {
        Ok(Sensitive::new(Url::parse("http://first_endpoint").unwrap())
            .with_redactor(to_safe_string))
    });
    base_layer
        .expect_get_proved_block_at()
        .times(1)
        .returning(|_| Ok(BlockHashAndNumber::default()));
    let mut wrapper = CyclicBaseLayerWrapper::new(
        base_layer,
        TEST_RETRY_PRIMARY_INTERVAL,
        ScraperLabel::L1Events,
    );

    // Test.
    let result = wrapper.get_proved_block_at(1).await;
    assert!(result.is_ok());
}

// Verifies that when already on the primary endpoint, retry_primary_if_due skips the reset even
// when the interval has elapsed (Duration::ZERO guarantees immediate elapse).
#[tokio::test]
async fn test_no_retry_primary_when_already_at_primary() {
    // Setup: report primary position; the timer is irrelevant since is_at_primary short-circuits.
    let mut base_layer = MockBaseLayerContract::new();
    base_layer.expect_is_at_primary().returning(|| Ok(true));
    base_layer.expect_reset_provider_url_to_primary().times(0);
    base_layer.expect_get_url().times(1).returning(|| {
        Ok(Sensitive::new(Url::parse("http://first_endpoint").unwrap())
            .with_redactor(to_safe_string))
    });
    base_layer
        .expect_get_proved_block_at()
        .times(1)
        .returning(|_| Ok(BlockHashAndNumber::default()));
    let mut wrapper =
        CyclicBaseLayerWrapper::new(base_layer, Duration::ZERO, ScraperLabel::L1Events);

    // Test.
    let result = wrapper.get_proved_block_at(1).await;
    assert!(result.is_ok());
}

// Verifies that cycling between backup endpoints does not push out the primary-retry clock:
// only the first failover that leaves the primary sets the clock, so the interval is measured
// from the moment we departed the primary, not from the latest backup-to-backup switch.
//
// Scenario (3 operations, paused tokio time):
//   Op1 @ T0:        starts on primary, fails once, cycles primary -> secondary (clock set to T0),
//                    then succeeds on secondary.
//   Op2 @ T0+30:     starts on secondary, fails once, cycles secondary -> tertiary
//                    (backup -> backup: clock must NOT move), then succeeds on tertiary.
//   Op3 @ T0+60:     starts on tertiary; retry_primary_if_due sees elapsed = 60 >= INTERVAL,
//                    so reset fires and we return to primary, where the call succeeds.
//
// If the bug were present (clock reset on every cycle), Op3 would see elapsed = 30 < INTERVAL
// and reset would NOT fire — so the `.times(1)` assertion on reset_provider_url_to_primary
// would fail, distinguishing fixed code from the buggy version.
#[tokio::test(start_paused = true)]
async fn test_retry_clock_not_reset_by_backup_cycles() {
    const INTERVAL: Duration = Duration::from_secs(60);

    let primary_url = Sensitive::new(Url::parse("http://primary_endpoint").unwrap())
        .with_redactor(to_safe_string);
    let secondary_url = Sensitive::new(Url::parse("http://secondary_endpoint").unwrap())
        .with_redactor(to_safe_string);
    let tertiary_url = Sensitive::new(Url::parse("http://tertiary_endpoint").unwrap())
        .with_redactor(to_safe_string);

    // is_at_primary return sequence (8 calls total):
    //   Op1 retry_primary_if_due:         true  (on primary, skip interval check)
    //   Op1 cycle_url_on_error error:     true  (was_at_primary → set clock to T0, emit gauge)
    //   Op1 cycle_url_on_error success:   false (on secondary → do not clear gauge)
    //   Op2 retry_primary_if_due:         false (on secondary, elapsed=30 < 60, no reset)
    //   Op2 cycle_url_on_error error:     false (backup → backup, clock must not move)
    //   Op2 cycle_url_on_error success:   false (on tertiary → do not clear gauge)
    //   Op3 retry_primary_if_due:         false (on tertiary, elapsed=60 >= 60, reset fires)
    //   Op3 cycle_url_on_error success:   true  (on primary after reset → clear gauge to 0)
    let mut is_at_primary_returns =
        [true, true, false, false, false, false, false, true].into_iter();

    // get_url return sequence (7 calls total):
    //   Op1: start=primary, current=primary, new=secondary
    //   Op2: start=secondary, current=secondary, new=tertiary
    //   Op3: start=primary  (after reset_provider_url_to_primary)
    let url_sequence = [
        primary_url.clone(),
        primary_url.clone(),
        secondary_url.clone(),
        secondary_url.clone(),
        secondary_url.clone(),
        tertiary_url.clone(),
        primary_url.clone(),
    ];
    let mut get_url_returns = url_sequence.into_iter();

    // get_proved_block_at return sequence (5 calls total):
    //   Op1: Err (primary fails), Ok (secondary succeeds)
    //   Op2: Err (secondary fails), Ok (tertiary succeeds)
    //   Op3: Ok  (primary succeeds after reset)
    let mut operation_returns: std::vec::IntoIter<Result<BlockHashAndNumber, MockError>> = vec![
        Err(MockError::MockError),
        Ok(BlockHashAndNumber::default()),
        Err(MockError::MockError),
        Ok(BlockHashAndNumber::default()),
        Ok(BlockHashAndNumber::default()),
    ]
    .into_iter();

    let mut base_layer = MockBaseLayerContract::new();
    base_layer
        .expect_is_at_primary()
        .times(8)
        .returning(move || Ok(is_at_primary_returns.next().unwrap()));
    base_layer.expect_get_url().times(7).returning(move || Ok(get_url_returns.next().unwrap()));
    base_layer
        .expect_get_proved_block_at()
        .times(5)
        .returning(move |_| operation_returns.next().unwrap());
    // Two cycles: primary -> secondary (Op1), secondary -> tertiary (Op2).
    base_layer.expect_cycle_provider_url().times(2).returning(|| Ok(()));
    // The key assertion: reset fires exactly once, at Op3. If backup cycles pushed the clock,
    // elapsed at T0+60 would be only 30 s (< INTERVAL) and this would fire zero times.
    base_layer.expect_reset_provider_url_to_primary().times(1).returning(|| Ok(()));

    let mut wrapper = CyclicBaseLayerWrapper::new(base_layer, INTERVAL, ScraperLabel::L1Events);

    // Op1 @ T0: primary fails, cycles to secondary, succeeds.
    let result = wrapper.get_proved_block_at(1).await;
    assert!(result.is_ok());

    // Advance to T0+30: half the interval has elapsed since leaving the primary.
    tokio::time::advance(Duration::from_secs(30)).await;

    // Op2 @ T0+30: secondary fails, cycles to tertiary (backup -> backup), succeeds.
    // The primary-retry clock must not move.
    let result = wrapper.get_proved_block_at(1).await;
    assert!(result.is_ok());

    // Advance to T0+60: the full interval has now elapsed since Op1 left the primary.
    tokio::time::advance(Duration::from_secs(30)).await;

    // Op3 @ T0+60: retry_primary_if_due fires, resets to primary, call succeeds.
    let result = wrapper.get_proved_block_at(1).await;
    assert!(result.is_ok());
}

// Verifies that retry_primary fires exactly once after the interval elapses. Uses
// start_paused = true for deterministic tokio time control.
#[tokio::test(start_paused = true)]
async fn test_retry_primary_fires_only_after_interval_elapses() {
    const INTERVAL: Duration = Duration::from_secs(60);

    let mut base_layer = MockBaseLayerContract::new();
    base_layer.expect_is_at_primary().returning(|| Ok(false));
    base_layer.expect_get_url().returning(|| {
        Ok(Sensitive::new(Url::parse("http://first_endpoint").unwrap())
            .with_redactor(to_safe_string))
    });
    // Two successful operation calls: one before the interval elapses, one after.
    base_layer
        .expect_get_proved_block_at()
        .times(2)
        .returning(|_| Ok(BlockHashAndNumber::default()));
    // Reset fires exactly once — when the second call runs after the interval has elapsed.
    base_layer.expect_reset_provider_url_to_primary().times(1).returning(|| Ok(()));
    let mut wrapper = CyclicBaseLayerWrapper::new(base_layer, INTERVAL, ScraperLabel::L1Events);

    // First call: elapsed is 0, which is less than INTERVAL, so no reset.
    let result = wrapper.get_proved_block_at(1).await;
    assert!(result.is_ok());

    // Advance time past the interval so the next call finds elapsed >= INTERVAL.
    tokio::time::advance(INTERVAL).await;

    // Second call: elapsed >= INTERVAL, so reset fires exactly once.
    let result = wrapper.get_proved_block_at(1).await;
    assert!(result.is_ok());
}

// A failed primary retry still advances the clock, so the next L1 access within the interval does
// not churn the primary again — it waits a full interval before retrying it.
#[tokio::test(start_paused = true)]
async fn test_retry_clock_advanced_when_reset_fails() {
    const INTERVAL: Duration = Duration::from_secs(60);

    let mut base_layer = MockBaseLayerContract::new();
    base_layer.expect_is_at_primary().returning(|| Ok(false));
    // Reset fires exactly once: the first access advances the clock even though the reset fails,
    // so the second access (still within the interval) does not retry the primary again.
    base_layer
        .expect_reset_provider_url_to_primary()
        .times(1)
        .returning(|| Err(MockError::MockError));
    base_layer.expect_get_url().returning(|| {
        Ok(Sensitive::new(Url::parse("http://first_endpoint").unwrap())
            .with_redactor(to_safe_string))
    });
    // The second access skips the retry and runs the operation, which succeeds.
    base_layer
        .expect_get_proved_block_at()
        .times(1)
        .returning(|_| Ok(BlockHashAndNumber::default()));
    let mut wrapper = CyclicBaseLayerWrapper::new(base_layer, INTERVAL, ScraperLabel::L1Events);

    // Make the retry due.
    tokio::time::advance(INTERVAL).await;

    // First access: retry is due, reset fails, so the operation errors before it runs.
    assert!(wrapper.get_proved_block_at(1).await.is_err());
    // The failed reset advanced the clock, so this access is no longer due: no second reset fires,
    // and the operation runs and succeeds.
    assert!(wrapper.get_proved_block_at(1).await.is_ok());
}

// Verifies the primary-down-since gauge:
// - After a failover away from the primary, the gauge is set to a nonzero unix timestamp.
// - After a successful call while back on the primary, the gauge is reset to 0.
#[tokio::test]
async fn test_primary_down_since_metric() {
    let primary_url = Sensitive::new(Url::parse("http://primary_endpoint").unwrap())
        .with_redactor(to_safe_string);
    let secondary_url = Sensitive::new(Url::parse("http://secondary_endpoint").unwrap())
        .with_redactor(to_safe_string);

    // is_at_primary return sequence (4 calls):
    //   Op1 retry_primary_if_due:  true  (on primary, skip interval check)
    //   Op1 cycle_url_on_error:    true  (was_at_primary → emit nonzero gauge)
    //   Op1 success path:          false (on secondary after cycle → do NOT clear gauge)
    //   Op2 retry_primary_if_due:  true  (on primary after reset; reset happens via
    //                                     retry_primary_if_due with Duration::ZERO interval)
    //   Op2 cycle_url_on_error success path: true → clear gauge to 0
    //
    // However, for simplicity we only test phase 1 (nonzero after failover).
    // is_at_primary calls for the failover scenario:
    //   retry_primary_if_due: true (1)
    //   cycle_url_on_error error path: true (was_at_primary) (1)
    //   cycle_url_on_error success path: false (on secondary) (1)
    let mut is_at_primary_returns = [true, true, false].into_iter();

    let url_sequence = [
        primary_url.clone(), // start_url
        primary_url.clone(), // current_url in error path
        secondary_url,       // new_url after cycle
    ];
    let mut get_url_returns = url_sequence.into_iter();

    let mut base_layer = MockBaseLayerContract::new();
    base_layer
        .expect_is_at_primary()
        .times(3)
        .returning(move || Ok(is_at_primary_returns.next().unwrap()));
    base_layer.expect_get_url().times(3).returning(move || Ok(get_url_returns.next().unwrap()));
    base_layer.expect_get_proved_block_at().times(1).returning(|_| Err(MockError::MockError));
    base_layer
        .expect_get_proved_block_at()
        .times(1)
        .returning(|_| Ok(BlockHashAndNumber::default()));
    base_layer.expect_cycle_provider_url().times(1).returning(|| Ok(()));

    // Install a thread-local Prometheus recorder so metric emissions are captured.
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    let prometheus_handle = recorder.handle();

    let mut wrapper = CyclicBaseLayerWrapper::new(
        base_layer,
        TEST_RETRY_PRIMARY_INTERVAL,
        ScraperLabel::L1Events,
    );

    // Trigger the failover: primary fails, cycle to secondary, secondary succeeds.
    let result = wrapper.get_proved_block_at(1).await;
    assert!(result.is_ok());

    // The gauge must now be set to a nonzero unix timestamp.
    let scraper_label_value: &'static str = ScraperLabel::L1Events.into();
    let metrics_str = prometheus_handle.render();
    let gauge_value = L1_PRIMARY_ENDPOINT_DOWN_SINCE_TIMESTAMP_SECONDS
        .parse_numeric_metric::<u64>(&metrics_str, &[(LABEL_NAME_SCRAPER, scraper_label_value)]);
    assert!(
        gauge_value.is_some_and(|timestamp| timestamp > 0),
        "Expected nonzero down-since timestamp after primary failover, got: {gauge_value:?}"
    );
}
