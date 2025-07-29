use std::sync::Arc;

use apollo_gateway_types::deprecated_gateway_error::{
    KnownStarknetErrorCode,
    StarknetError,
    StarknetErrorCode,
};
use apollo_mempool_types::communication::MockMempoolClient;
use blockifier::blockifier::stateful_validator::{
    MockStatefulValidatorTrait as MockBlockifierStatefulValidatorTrait,
    StatefulValidatorError as BlockifierStatefulValidatorError,
};
use blockifier::context::ChainInfo;
use blockifier::test_utils::contracts::FeatureContractTrait;
use blockifier::transaction::errors::{TransactionFeeError, TransactionPreValidationError};
use blockifier::transaction::test_utils::calculate_class_info_for_testing;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use mempool_test_utils::starknet_api_test_utils::{
    executable_invoke_tx as create_executable_invoke_tx,
    VALID_L1_GAS_MAX_AMOUNT,
    VALID_L1_GAS_MAX_PRICE_PER_UNIT,
};
use mockall::predicate::eq;
use num_bigint::BigUint;
use pretty_assertions::assert_eq;
use rstest::{fixture, rstest};
use starknet_api::block::{BlockInfo, GasPrice, GasPriceVector, GasPrices, NonzeroGasPrice};
use starknet_api::core::Nonce;
use starknet_api::executable_transaction::AccountTransaction;
use starknet_api::execution_resources::GasAmount;
use starknet_api::test_utils::declare::executable_declare_tx;
use starknet_api::test_utils::deploy_account::executable_deploy_account_tx;
use starknet_api::test_utils::invoke::executable_invoke_tx;
use starknet_api::transaction::fields::{
    AllResourceBounds,
    Resource,
    ResourceBounds,
    ValidResourceBounds,
};
use starknet_api::{declare_tx_args, deploy_account_tx_args, invoke_tx_args, nonce};

use crate::config::StatefulTransactionValidatorConfig;
use crate::state_reader::{MockStateReaderFactory, StateReaderFactory};
use crate::state_reader_test_utils::local_test_state_reader_factory;
use crate::stateful_transaction_validator::StatefulTransactionValidator;

#[fixture]
fn stateful_validator() -> StatefulTransactionValidator {
    StatefulTransactionValidator { config: StatefulTransactionValidatorConfig::default() }
}

// TODO(Arni): consider testing declare and deploy account.
#[rstest]
#[case::valid_tx(create_executable_invoke_tx(CairoVersion::Cairo1(RunnableCairo1::Casm)), true)]
#[case::invalid_tx(create_executable_invoke_tx(CairoVersion::Cairo1(RunnableCairo1::Casm)), false)]
#[tokio::test]
async fn test_stateful_tx_validator(
    #[case] executable_tx: AccountTransaction,
    #[case] expect_ok: bool,
    stateful_validator: StatefulTransactionValidator,
) {
    let expected_result = if expect_ok {
        Ok(())
    } else {
        Err(BlockifierStatefulValidatorError::TransactionPreValidationError(
            TransactionPreValidationError::TransactionFeeError(Box::new(
                TransactionFeeError::GasBoundsExceedBalance {
                    resource: Resource::L1DataGas,
                    max_amount: GasAmount(VALID_L1_GAS_MAX_AMOUNT),
                    max_price: GasPrice(VALID_L1_GAS_MAX_PRICE_PER_UNIT),
                    balance: BigUint::ZERO,
                },
            )),
        ))
    };
    let expected_result_as_stateful_transaction_result = expected_result
        .as_ref()
        .map(|validate_result| *validate_result)
        .map_err(|blockifier_error| StarknetError {
            code: StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::ValidateFailure),
            message: format!("{blockifier_error}"),
        });

    let mut mock_blockifier_validator = MockBlockifierStatefulValidatorTrait::new();
    mock_blockifier_validator.expect_validate().return_once(|_| expected_result.map(|_| ()));
    mock_blockifier_validator.expect_block_info().return_const(BlockInfo::default());

    let account_nonce = nonce!(0);
    let mut mock_mempool_client = MockMempoolClient::new();
    mock_mempool_client.expect_account_tx_in_pool_or_recent_block().returning(|_| {
        // The mempool does not have any transactions from the sender.
        Ok(false)
    });
    let mempool_client = Arc::new(mock_mempool_client);
    let runtime = tokio::runtime::Handle::current();

    let result = tokio::task::spawn_blocking(move || {
        stateful_validator.run_transaction_validations(
            &executable_tx,
            account_nonce,
            mempool_client,
            mock_blockifier_validator,
            runtime,
        )
    })
    .await
    .unwrap();
    assert_eq!(result, expected_result_as_stateful_transaction_result);
}

#[rstest]
fn test_instantiate_validator(stateful_validator: StatefulTransactionValidator) {
    let state_reader_factory =
        local_test_state_reader_factory(CairoVersion::Cairo1(RunnableCairo1::Casm), false);

    let mut mock_state_reader_factory = MockStateReaderFactory::new();

    // Make sure stateful_validator uses the latest block in the initial call.
    let latest_state_reader = state_reader_factory.get_state_reader_from_latest_block();
    mock_state_reader_factory
        .expect_get_state_reader_from_latest_block()
        .return_once(|| latest_state_reader);

    // Make sure stateful_validator uses the latest block in the following calls to the
    // state_reader.
    let latest_block = state_reader_factory.state_reader.block_info.block_number;
    let state_reader = state_reader_factory.get_state_reader(latest_block);
    mock_state_reader_factory
        .expect_get_state_reader()
        .with(eq(latest_block))
        .return_once(move |_| state_reader);

    let blockifier_validator = stateful_validator
        .instantiate_validator(&mock_state_reader_factory, &ChainInfo::create_for_testing());
    assert!(blockifier_validator.is_ok());
}

#[rstest]
#[case::should_skip_validation(
    executable_invoke_tx(invoke_tx_args!(nonce: nonce!(1))),
    nonce!(0),
    true,
    false
)]
#[case::should_not_skip_validation_nonce_zero(
    executable_invoke_tx(invoke_tx_args!(nonce: nonce!(0))),
    nonce!(0),
    true,
    true
)]
#[case::should_not_skip_validation_nonce_over_one(
    executable_invoke_tx(invoke_tx_args!(nonce: nonce!(2))),
    nonce!(0),
    true,
    true
)]
// TODO(Arni): Fix this test case. Ideally, we would have a non-invoke transaction with tx_nonce 1
// and account_nonce 0. For deploy account the tx_nonce is always 0. Replace with a declare tx.
#[case::should_not_skip_validation_non_invoke(
    executable_deploy_account_tx(deploy_account_tx_args!()),
    nonce!(0),
    true,
    true

)]
#[case::should_not_skip_validation_account_nonce_1(
    executable_invoke_tx(invoke_tx_args!(nonce: nonce!(1))),
    nonce!(1),
    true,
    true
)]
#[case::should_not_skip_validation_no_tx_in_mempool(
    executable_invoke_tx(invoke_tx_args!(nonce: nonce!(1))),
    nonce!(0),
    false,
    true
)]
#[tokio::test]
async fn test_skip_validate(
    #[case] executable_tx: AccountTransaction,
    #[case] sender_nonce: Nonce,
    #[case] contains_tx: bool,
    #[case] should_validate: bool,
    stateful_validator: StatefulTransactionValidator,
) {
    let mut mock_blockifier_validator = MockBlockifierStatefulValidatorTrait::new();
    mock_blockifier_validator
        .expect_validate()
        .withf(move |tx| tx.execution_flags.validate == should_validate)
        .returning(|_| Ok(()));
    mock_blockifier_validator.expect_block_info().return_const(BlockInfo::default());
    let mut mock_mempool_client = MockMempoolClient::new();
    mock_mempool_client
        .expect_account_tx_in_pool_or_recent_block()
        .returning(move |_| Ok(contains_tx));
    let mempool_client = Arc::new(mock_mempool_client);
    let runtime = tokio::runtime::Handle::current();

    tokio::task::spawn_blocking(move || {
        let _ = stateful_validator.run_transaction_validations(
            &executable_tx,
            sender_nonce,
            mempool_client,
            mock_blockifier_validator,
            runtime,
        );
    })
    .await
    .unwrap();
}

#[rstest]
#[case::tx_gas_price_meets_threshold_exactly_pass(
    100_u128.try_into().unwrap(),
    100,
    100_u128.into(),
    Ok(())
)]
#[case::tx_gas_price_below_threshold_fail(
    100_u128.try_into().unwrap(),
    100,
    99_u128.into(),
    Err(StarknetError {
        code: StarknetErrorCode::UnknownErrorCode(
            "StarknetErrorCode.GAS_PRICE_TOO_LOW".to_string(),
        ),
        message: "Transaction L2 gas price 99 is below the required threshold 100.".to_string(),
    })
)]
#[case::tx_gas_price_meets_threshold_with_factor_pass(
    100_u128.try_into().unwrap(),
    50,
    50_u128.into(),
    Ok(())
)]
#[case::tx_gas_price_above_threshold_with_factor_pass(
    100_u128.try_into().unwrap(),
    50,
    51_u128.into(),
    Ok(())
)]
#[case::tx_gas_price_below_threshold_with_factor_fail(
    100_u128.try_into().unwrap(),
    50,
    49_u128.into(),
    Err(StarknetError {
        code: StarknetErrorCode::UnknownErrorCode(
            "StarknetErrorCode.GAS_PRICE_TOO_LOW".to_string(),
        ),
        message: "Transaction L2 gas price 49 is below the required threshold 50.".to_string(),
    })
)]
#[case::gas_price_check_disabled_when_percentage_zero_pass(
    100_u128.try_into().unwrap(),
    0,
    0_u128.into(),
    Ok(())
)]
#[case::tx_gas_price_zero_fails_when_percentage_nonzero_fail(
    100_u128.try_into().unwrap(),
    10,
    0_u128.into(),
    Err(StarknetError {
        code: StarknetErrorCode::UnknownErrorCode(
            "StarknetErrorCode.GAS_PRICE_TOO_LOW".to_string(),
        ),
        message: "Transaction L2 gas price 0 is below the required threshold 10.".to_string(),
    })
)]
#[tokio::test]
async fn validate_resource_bounds(
    #[case] prev_l2_gas_price: NonzeroGasPrice,
    #[case] min_gas_price_percentage: u8,
    #[case] tx_gas_price_per_unit: GasPrice,
    #[case] expected_result: Result<(), StarknetError>,
    mut stateful_validator: StatefulTransactionValidator,
) {
    stateful_validator.config.validate_resource_bounds = true;
    stateful_validator.config.min_gas_price_percentage = min_gas_price_percentage;

    let resource_bounds = ValidResourceBounds::AllResources(AllResourceBounds {
        l2_gas: ResourceBounds { max_price_per_unit: tx_gas_price_per_unit, ..Default::default() },
        ..Default::default()
    });
    let executable_tx = executable_invoke_tx(invoke_tx_args!(resource_bounds));

    let mut mock_blockifier_validator = MockBlockifierStatefulValidatorTrait::new();
    mock_blockifier_validator.expect_validate().return_once(|_| Ok(()));
    mock_blockifier_validator.expect_block_info().return_const(BlockInfo {
        gas_prices: GasPrices {
            strk_gas_prices: GasPriceVector {
                l2_gas_price: prev_l2_gas_price,
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    });

    let result = tokio::task::spawn_blocking(move || {
        stateful_validator.run_transaction_validations(
            &executable_tx,
            nonce!(0),
            Arc::new(MockMempoolClient::new()),
            mock_blockifier_validator,
            tokio::runtime::Handle::current(),
        )
    })
    .await
    .unwrap();
    assert_eq!(result, expected_result);
}

#[rstest]
#[case::nonce_equal_to_account_nonce(0, 1, 1, Ok(()))]
#[case::nonce_in_allowed_range(10, 1, 11, Ok(()))]
#[case::nonce_beyond_allowed_gap(
    10,
    1,
    12,
    Err(StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::InvalidTransactionNonce))
)]
#[case::nonce_less_then_account_nonce(
    0,
    1,
    0,
    Err(StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::InvalidTransactionNonce))
)]
#[tokio::test]
async fn test_is_valid_nonce(
    #[case] max_allowed_nonce_gap: u32,
    #[case] account_nonce: u32,
    #[case] tx_nonce: u32,
    #[case] expected_result_code: Result<(), StarknetErrorCode>,
) {
    let stateful_validator = StatefulTransactionValidator {
        config: StatefulTransactionValidatorConfig { max_allowed_nonce_gap, ..Default::default() },
    };
    let mut mock_blockifier_validator = MockBlockifierStatefulValidatorTrait::new();
    mock_blockifier_validator.expect_validate().return_once(|_| Ok(()));
    mock_blockifier_validator.expect_block_info().return_const(BlockInfo::default());
    let executable_tx = executable_invoke_tx(invoke_tx_args!(
        nonce: nonce!(tx_nonce),
        resource_bounds: ValidResourceBounds::create_for_testing(),
    ));

    let result = tokio::task::spawn_blocking(move || {
        stateful_validator.run_transaction_validations(
            &executable_tx,
            nonce!(account_nonce),
            Arc::new(MockMempoolClient::new()),
            mock_blockifier_validator,
            tokio::runtime::Handle::current(),
        )
    })
    .await
    .unwrap()
    .map_err(|err| err.code);
    assert_eq!(result, expected_result_code);
}

#[rstest]
#[case::nonce_equal_to_account_nonce(0, Ok(()))]
#[case::nonce_greater_then_account_nonce(
    1,
    Err(StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::InvalidTransactionNonce))
)]
#[case::nonce_less_then_account_nonce(-1, Err(StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::InvalidTransactionNonce)))]
#[tokio::test]
async fn test_reject_future_declares(
    stateful_validator: StatefulTransactionValidator,
    #[case] account_nonce_diff: i32,
    #[case] expected_result_code: Result<(), StarknetErrorCode>,
) {
    let mut mock_blockifier_validator = MockBlockifierStatefulValidatorTrait::new();
    mock_blockifier_validator.expect_validate().return_once(|_| Ok(()));
    mock_blockifier_validator.expect_block_info().return_const(BlockInfo::default());

    let account_nonce = 10;
    let executable_tx = executable_declare_tx(
        declare_tx_args!(
            nonce: nonce!(account_nonce + account_nonce_diff),
            resource_bounds: ValidResourceBounds::create_for_testing(),
        ),
        calculate_class_info_for_testing(
            FeatureContract::Empty(CairoVersion::Cairo1(RunnableCairo1::Casm)).get_class(),
        ),
    );

    let result = tokio::task::spawn_blocking(move || {
        stateful_validator.run_transaction_validations(
            &executable_tx,
            nonce!(account_nonce),
            Arc::new(MockMempoolClient::new()),
            mock_blockifier_validator,
            tokio::runtime::Handle::current(),
        )
    })
    .await
    .unwrap()
    .map_err(|err| err.code);
    assert_eq!(result, expected_result_code);
}
