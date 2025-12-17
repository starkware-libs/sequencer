use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use rstest::rstest;
use starknet_api::block::BlockHashAndNumber;
use url::Url;

use crate::cyclic_base_layer_wrapper::CyclicBaseLayerWrapper;
use crate::{BaseLayerContract, L1BlockHeader, L1BlockReference, MockBaseLayerContract, MockError};

fn get_url_helper(num_url_calls_made: &AtomicUsize) -> Result<Url, MockError> {
    if num_url_calls_made.load(Ordering::Relaxed).is_multiple_of(2) {
        Ok(Url::parse("http://first_endpoint").unwrap())
    } else {
        Ok(Url::parse("http://second_endpoint").unwrap())
    }
}

// In the following tests, we specify the number of failed calls, each to a different URL endpoint.
// Once the number of failures are reached, we expect to get a success or a failure, depending on
// whether there are any URLs left to try. We use a base layer with two URLs that get cycled.

const NUM_URLS: usize = 2;

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
    let mut wrapper = CyclicBaseLayerWrapper::new(base_layer);

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
    let mut wrapper = CyclicBaseLayerWrapper::new(base_layer);

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
    let mut wrapper = CyclicBaseLayerWrapper::new(base_layer);

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
    let mut wrapper = CyclicBaseLayerWrapper::new(base_layer);

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
    let mut wrapper = CyclicBaseLayerWrapper::new(base_layer);

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
    let mut wrapper = CyclicBaseLayerWrapper::new(base_layer);

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
    base_layer
        .expect_get_url()
        .times(2)
        .returning(move || Ok(Url::parse("http://first_endpoint").unwrap()));
    base_layer.expect_get_block_header().times(1).returning(move |_| Err(MockError::MockError));
    base_layer.expect_cycle_provider_url().times(1).returning(move || Err(MockError::MockError));
    let mut wrapper = CyclicBaseLayerWrapper::new(base_layer);

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
    let wrapper = CyclicBaseLayerWrapper::new(base_layer);

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
    base_layer.expect_get_url().returning(move || Ok(Url::parse("http://first_endpoint").unwrap()));
    let wrapper = CyclicBaseLayerWrapper::new(base_layer);

    // Test.
    let result = wrapper.get_url().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn pass_through_set_provider_url() {
    // Setup.
    let mut base_layer = MockBaseLayerContract::new();
    base_layer.expect_set_provider_url().returning(move |_| Ok(()));
    let mut wrapper = CyclicBaseLayerWrapper::new(base_layer);

    // Test.
    let result = wrapper.set_provider_url(Url::parse("http://first_endpoint").unwrap()).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn pass_through_cycle_provider_url() {
    // Setup.
    let mut base_layer = MockBaseLayerContract::new();
    base_layer.expect_cycle_provider_url().returning(move || Ok(()));
    let mut wrapper = CyclicBaseLayerWrapper::new(base_layer);

    // Test.
    let result = wrapper.cycle_provider_url().await;
    assert!(result.is_ok());
}
