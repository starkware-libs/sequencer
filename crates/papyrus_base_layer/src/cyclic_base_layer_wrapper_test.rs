use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use apollo_config::secrets::Sensitive;
use apollo_infra_utils::url::to_safe_string;
use rstest::rstest;
use starknet_api::block::BlockHashAndNumber;
use url::Url;

use crate::cyclic_base_layer_wrapper::CyclicBaseLayerWrapper;
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
    let mut wrapper = CyclicBaseLayerWrapper::new(base_layer, TEST_RETRY_PRIMARY_INTERVAL);

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
    let mut wrapper = CyclicBaseLayerWrapper::new(base_layer, TEST_RETRY_PRIMARY_INTERVAL);

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
    let mut wrapper = CyclicBaseLayerWrapper::new(base_layer, TEST_RETRY_PRIMARY_INTERVAL);

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
    let mut wrapper = CyclicBaseLayerWrapper::new(base_layer, TEST_RETRY_PRIMARY_INTERVAL);

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
    let mut wrapper = CyclicBaseLayerWrapper::new(base_layer, TEST_RETRY_PRIMARY_INTERVAL);

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

    base_layer.expect_get_url().times(1).returning(move || Err(MockError::MockError));
    let mut wrapper = CyclicBaseLayerWrapper::new(base_layer, TEST_RETRY_PRIMARY_INTERVAL);

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
    base_layer.expect_get_url().times(2).returning(move || {
        Ok(Sensitive::new(Url::parse("http://first_endpoint").unwrap())
            .with_redactor(to_safe_string))
    });
    base_layer.expect_get_block_header().times(1).returning(move |_| Err(MockError::MockError));
    base_layer.expect_cycle_provider_url().times(1).returning(move || Err(MockError::MockError));
    let mut wrapper = CyclicBaseLayerWrapper::new(base_layer, TEST_RETRY_PRIMARY_INTERVAL);

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
    let wrapper = CyclicBaseLayerWrapper::new(base_layer, TEST_RETRY_PRIMARY_INTERVAL);

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
    let wrapper = CyclicBaseLayerWrapper::new(base_layer, TEST_RETRY_PRIMARY_INTERVAL);

    // Test.
    let result = wrapper.get_url().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn pass_through_set_provider_url() {
    // Setup.
    let mut base_layer = MockBaseLayerContract::new();
    base_layer.expect_set_provider_url().returning(move |_| Ok(()));
    let mut wrapper = CyclicBaseLayerWrapper::new(base_layer, TEST_RETRY_PRIMARY_INTERVAL);

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
    let mut wrapper = CyclicBaseLayerWrapper::new(base_layer, TEST_RETRY_PRIMARY_INTERVAL);

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
    let mut wrapper = CyclicBaseLayerWrapper::new(base_layer, TEST_RETRY_PRIMARY_INTERVAL);

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
    base_layer.expect_reset_provider_url_to_primary().times(1).returning(|| Ok(()));
    base_layer.expect_get_url().times(1).returning(|| {
        Ok(Sensitive::new(Url::parse("http://first_endpoint").unwrap())
            .with_redactor(to_safe_string))
    });
    base_layer
        .expect_get_proved_block_at()
        .times(1)
        .returning(|_| Ok(BlockHashAndNumber::default()));
    let mut wrapper = CyclicBaseLayerWrapper::new(base_layer, Duration::ZERO);

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
    base_layer
        .expect_reset_provider_url_to_primary()
        .times(1)
        .returning(|| Err(MockError::MockError));
    // The underlying operation must never be reached when retry-primary errors out.
    base_layer.expect_get_proved_block_at().times(0);

    let mut wrapper = CyclicBaseLayerWrapper::new(base_layer, Duration::ZERO);

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
    base_layer.expect_reset_provider_url_to_primary().times(0);
    base_layer.expect_get_url().times(1).returning(|| {
        Ok(Sensitive::new(Url::parse("http://first_endpoint").unwrap())
            .with_redactor(to_safe_string))
    });
    base_layer
        .expect_get_proved_block_at()
        .times(1)
        .returning(|_| Ok(BlockHashAndNumber::default()));
    let mut wrapper = CyclicBaseLayerWrapper::new(base_layer, TEST_RETRY_PRIMARY_INTERVAL);

    // Test.
    let result = wrapper.get_proved_block_at(1).await;
    assert!(result.is_ok());
}
