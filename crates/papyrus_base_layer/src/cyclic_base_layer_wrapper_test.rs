use rstest::rstest;
use starknet_api::block::BlockHashAndNumber;
use url::Url;

use crate::cyclic_base_layer_wrapper::CyclicBaseLayerWrapper;
use crate::{BaseLayerContract, L1BlockHeader, L1BlockReference, MockBaseLayerContract, MockError};

#[rstest]
#[case::success(1, 0, 1, true)]
#[case::fail_first(1, 1, 2, true)]
#[case::fail_both(0, 2, 3, false)]
#[tokio::test]
async fn cycle_get_proved_block_at(
    #[case] num_successfull_calls: usize,
    #[case] num_failing_calls: usize,
    #[case] num_url_calls: usize,
    #[case] success: bool,
) {
    // Setup.
    let mut base_layer = MockBaseLayerContract::new();
    base_layer
        .expect_get_proved_block_at()
        .times(num_failing_calls)
        .returning(move |_| Err(MockError::MockError));
    base_layer
        .expect_get_proved_block_at()
        .times(num_successfull_calls)
        .returning(move |_| Ok(BlockHashAndNumber::default()));
    base_layer
        .expect_get_url()
        .times(usize::from(num_url_calls >= 1))
        .returning(move || Ok(Url::parse("http://first_endpoint").unwrap()));
    base_layer
        .expect_get_url()
        .times(usize::from(num_url_calls >= 2))
        .returning(move || Ok(Url::parse("http://second_endpoint").unwrap()));
    base_layer
        .expect_get_url()
        .times(usize::from(num_url_calls >= 3))
        .returning(move || Ok(Url::parse("http://first_endpoint").unwrap()));
    base_layer.expect_cycle_provider_url().times(num_url_calls - 1).returning(move || Ok(()));
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
#[case::success(1, 0, 1, true)]
#[case::fail_first(1, 1, 2, true)]
#[case::fail_both(0, 2, 3, false)]
#[tokio::test]
async fn cycle_latest_l1_block_number(
    #[case] num_successfull_calls: usize,
    #[case] num_failing_calls: usize,
    #[case] num_url_calls: usize,
    #[case] success: bool,
) {
    // Setup.
    let mut base_layer = MockBaseLayerContract::new();
    base_layer
        .expect_latest_l1_block_number()
        .times(num_failing_calls)
        .returning(move || Err(MockError::MockError));
    base_layer
        .expect_latest_l1_block_number()
        .times(num_successfull_calls)
        .returning(move || Ok(1));
    base_layer
        .expect_get_url()
        .times(usize::from(num_url_calls >= 1))
        .returning(move || Ok(Url::parse("http://first_endpoint").unwrap()));
    base_layer
        .expect_get_url()
        .times(usize::from(num_url_calls >= 2))
        .returning(move || Ok(Url::parse("http://second_endpoint").unwrap()));
    base_layer
        .expect_get_url()
        .times(usize::from(num_url_calls >= 3))
        .returning(move || Ok(Url::parse("http://first_endpoint").unwrap()));
    base_layer.expect_cycle_provider_url().times(num_url_calls - 1).returning(move || Ok(()));
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
#[case::success(1, 0, 1, true)]
#[case::fail_first(1, 1, 2, true)]
#[case::fail_both(0, 2, 3, false)]
#[tokio::test]
async fn cycle_l1_block_at(
    #[case] num_successfull_calls: usize,
    #[case] num_failing_calls: usize,
    #[case] num_url_calls: usize,
    #[case] success: bool,
) {
    // Setup.
    let mut base_layer = MockBaseLayerContract::new();
    base_layer
        .expect_l1_block_at()
        .times(num_failing_calls)
        .returning(move |_| Err(MockError::MockError));
    base_layer
        .expect_l1_block_at()
        .times(num_successfull_calls)
        .returning(move |_| Ok(Some(L1BlockReference::default())));
    base_layer
        .expect_get_url()
        .times(usize::from(num_url_calls >= 1))
        .returning(move || Ok(Url::parse("http://first_endpoint").unwrap()));
    base_layer
        .expect_get_url()
        .times(usize::from(num_url_calls >= 2))
        .returning(move || Ok(Url::parse("http://second_endpoint").unwrap()));
    base_layer
        .expect_get_url()
        .times(usize::from(num_url_calls >= 3))
        .returning(move || Ok(Url::parse("http://first_endpoint").unwrap()));
    base_layer.expect_cycle_provider_url().times(num_url_calls - 1).returning(move || Ok(()));
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
#[case::success(1, 0, 1, true)]
#[case::fail_first(1, 1, 2, true)]
#[case::fail_both(0, 2, 3, false)]
#[tokio::test]
async fn cycle_events(
    #[case] num_successfull_calls: usize,
    #[case] num_failing_calls: usize,
    #[case] num_url_calls: usize,
    #[case] success: bool,
) {
    // Setup.
    let mut base_layer = MockBaseLayerContract::new();
    base_layer
        .expect_events()
        .times(num_failing_calls)
        .returning(move |_, _| Err(MockError::MockError));
    base_layer.expect_events().times(num_successfull_calls).returning(move |_, _| Ok(vec![]));
    base_layer
        .expect_get_url()
        .times(usize::from(num_url_calls >= 1))
        .returning(move || Ok(Url::parse("http://first_endpoint").unwrap()));
    base_layer
        .expect_get_url()
        .times(usize::from(num_url_calls >= 2))
        .returning(move || Ok(Url::parse("http://second_endpoint").unwrap()));
    base_layer
        .expect_get_url()
        .times(usize::from(num_url_calls >= 3))
        .returning(move || Ok(Url::parse("http://first_endpoint").unwrap()));
    base_layer.expect_cycle_provider_url().times(num_url_calls - 1).returning(move || Ok(()));
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
#[case::success(1, 0, 1, true)]
#[case::fail_first(1, 1, 2, true)]
#[case::fail_both(0, 2, 3, false)]
#[tokio::test]
async fn cycle_get_block_header(
    #[case] num_successfull_calls: usize,
    #[case] num_failing_calls: usize,
    #[case] num_url_calls: usize,
    #[case] success: bool,
) {
    // Setup.
    let mut base_layer = MockBaseLayerContract::new();
    base_layer
        .expect_get_block_header()
        .times(num_failing_calls)
        .returning(move |_| Err(MockError::MockError));
    base_layer
        .expect_get_block_header()
        .times(num_successfull_calls)
        .returning(move |_| Ok(Some(L1BlockHeader::default())));
    base_layer
        .expect_get_url()
        .times(usize::from(num_url_calls >= 1))
        .returning(move || Ok(Url::parse("http://first_endpoint").unwrap()));
    base_layer
        .expect_get_url()
        .times(usize::from(num_url_calls >= 2))
        .returning(move || Ok(Url::parse("http://second_endpoint").unwrap()));
    base_layer
        .expect_get_url()
        .times(usize::from(num_url_calls >= 3))
        .returning(move || Ok(Url::parse("http://first_endpoint").unwrap()));
    base_layer.expect_cycle_provider_url().times(num_url_calls - 1).returning(move || Ok(()));
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
