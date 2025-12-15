use std::sync::Arc;

use apollo_gateway_config::config::StatefulTransactionValidatorConfig;
use apollo_gateway_types::deprecated_gateway_error::{
    KnownStarknetErrorCode,
    StarknetError,
    StarknetErrorCode,
};
use apollo_mempool_types::communication::MockMempoolClient;
use blockifier::blockifier::config::ContractClassManagerConfig;
use blockifier::context::ChainInfo;
use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier::test_utils::contracts::FeatureContractTrait;
use blockifier::transaction::test_utils::calculate_class_info_for_testing;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use mockall::predicate::eq;
use pretty_assertions::assert_eq;
use rstest::rstest;
use starknet_api::block::{BlockInfo, GasPrice, GasPriceVector, GasPrices, NonzeroGasPrice};
use starknet_api::core::Nonce;
use starknet_api::executable_transaction::AccountTransaction;
use starknet_api::test_utils::declare::executable_declare_tx;
use starknet_api::test_utils::deploy_account::executable_deploy_account_tx;
use starknet_api::test_utils::invoke::executable_invoke_tx;
use starknet_api::transaction::fields::{AllResourceBounds, ResourceBounds, ValidResourceBounds};
use starknet_api::{declare_tx_args, deploy_account_tx_args, invoke_tx_args, nonce};

use crate::gateway_fixed_block_state_reader::MockGatewayFixedBlockStateReader;
use crate::state_reader_test_utils::local_test_state_reader_factory;
use crate::stateful_transaction_validator::{
    StatefulTransactionValidator,
    StatefulTransactionValidatorFactory,
    StatefulTransactionValidatorFactoryTrait,
    StatefulTransactionValidatorTrait,
};

#[tokio::test]
async fn test_get_nonce_fail_on_extract_state_nonce_and_run_validations() {
    let executable_tx = executable_invoke_tx(invoke_tx_args!());
    let mut mock_gateway_fixed_block = MockGatewayFixedBlockStateReader::new();
    mock_gateway_fixed_block
        .expect_get_nonce()
        .with(eq(executable_tx.sender_address()))
        .return_once(|_| {
            Err(StarknetError {
                code: StarknetErrorCode::UnknownErrorCode(
                    "StarknetErrorCode.InternalError".to_string(),
                ),
                message: "Internal error".to_string(),
            })
        });

    let mempool_client = Arc::new(MockMempoolClient::new());
    let mut stateful_validator = StatefulTransactionValidator {
        config: StatefulTransactionValidatorConfig::default(),
        chain_info: ChainInfo::create_for_testing(),
        state_reader_and_contract_manager: None,
        gateway_fixed_block_state_reader: Box::new(mock_gateway_fixed_block),
    };

    let result = stateful_validator
        .extract_state_nonce_and_run_validations(&executable_tx, mempool_client)
        .await;
    assert_eq!(
        result,
        Err(StarknetError {
            code: StarknetErrorCode::UnknownErrorCode(
                "StarknetErrorCode.InternalError".to_string()
            ),
            message: "Internal error".to_string(),
        })
    );
}

// TODO(Arni): consider testing declare and deploy account.
#[rstest]
#[case::valid_tx(false, Ok(false))]
#[case::invalid_tx(
    true,
    Err(StarknetError {
        code: StarknetErrorCode::UnknownErrorCode(
            "StarknetErrorCode.GAS_PRICE_TOO_LOW".to_string(),
        ),
        message: "Transaction L2 gas price 0 is below the required threshold 1.".to_string(),
    })
)]
#[tokio::test]
async fn test_run_pre_validation_checks(
    #[case] zero_gas_fee: bool,
    #[case] expected_result: Result<bool, StarknetError>,
) {
    let account_nonce = nonce!(0);

    let mut mock_mempool_client = MockMempoolClient::new();
    mock_mempool_client.expect_account_tx_in_pool_or_recent_block().returning(|_| {
        // The mempool does not have any transactions from the sender.
        Ok(false)
    });
    mock_mempool_client.expect_validate_tx().returning(|_| Ok(()));
    let mempool_client = Arc::new(mock_mempool_client);

    let mut mock_gateway_fixed_block = MockGatewayFixedBlockStateReader::new();
    mock_gateway_fixed_block.expect_get_block_info().returning(|| Ok(BlockInfo::default()));

    let stateful_validator = StatefulTransactionValidator {
        config: StatefulTransactionValidatorConfig::default(),
        chain_info: ChainInfo::create_for_testing(),
        state_reader_and_contract_manager: None,
        gateway_fixed_block_state_reader: Box::new(mock_gateway_fixed_block),
    };

    let resource_bounds = if zero_gas_fee {
        ValidResourceBounds::AllResources(AllResourceBounds {
            l2_gas: ResourceBounds { max_price_per_unit: 0_u128.into(), ..Default::default() },
            ..Default::default()
        })
    } else {
        ValidResourceBounds::create_for_testing()
    };
    let executable_tx: AccountTransaction = executable_invoke_tx(invoke_tx_args!(resource_bounds));

    let result = stateful_validator
        .run_pre_validation_checks(&executable_tx, account_nonce, mempool_client)
        .await;
    assert_eq!(result, expected_result);
}

#[rstest]
#[tokio::test]
async fn test_instantiate_validator() {
    let state_reader_factory =
        local_test_state_reader_factory(CairoVersion::Cairo1(RunnableCairo1::Casm), false);

    let stateful_validator_factory = StatefulTransactionValidatorFactory {
        config: StatefulTransactionValidatorConfig::default(),
        chain_info: ChainInfo::create_for_testing(),
        state_reader_factory: Arc::new(state_reader_factory),
        contract_class_manager: ContractClassManager::start(ContractClassManagerConfig::default()),
    };

    let validator = stateful_validator_factory.instantiate_validator().await;
    assert!(validator.is_ok());
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
) {
    let mut mock_mempool_client = MockMempoolClient::new();
    mock_mempool_client
        .expect_account_tx_in_pool_or_recent_block()
        .returning(move |_| Ok(contains_tx));
    mock_mempool_client.expect_validate_tx().returning(|_| Ok(()));
    let mempool_client = Arc::new(mock_mempool_client);

    // Configure gateway state reader to return the provided sender/account nonce.
    let mut mock_gateway_fixed_block = MockGatewayFixedBlockStateReader::new();
    mock_gateway_fixed_block
        .expect_get_nonce()
        .with(eq(executable_tx.sender_address()))
        .return_once(move |_| Ok(sender_nonce));
    let stateful_validator = StatefulTransactionValidator {
        config: StatefulTransactionValidatorConfig {
            validate_resource_bounds: false,
            ..Default::default()
        },
        chain_info: ChainInfo::create_for_testing(),
        state_reader_and_contract_manager: None,
        gateway_fixed_block_state_reader: Box::new(mock_gateway_fixed_block),
    };

    let skip_validate = stateful_validator
        .run_pre_validation_checks(&executable_tx, sender_nonce, mempool_client)
        .await
        .unwrap();
    assert_eq!(skip_validate, !should_validate);
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
    Ok(()),
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
) {
    let resource_bounds = ValidResourceBounds::AllResources(AllResourceBounds {
        l2_gas: ResourceBounds { max_price_per_unit: tx_gas_price_per_unit, ..Default::default() },
        ..Default::default()
    });
    let executable_tx = executable_invoke_tx(invoke_tx_args!(resource_bounds));

    let mut mock_gateway_fixed_block = MockGatewayFixedBlockStateReader::new();
    mock_gateway_fixed_block.expect_get_block_info().return_once(move || {
        Ok(BlockInfo {
            gas_prices: GasPrices {
                strk_gas_prices: GasPriceVector {
                    l2_gas_price: prev_l2_gas_price,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        })
    });

    let stateful_validator = StatefulTransactionValidator {
        config: StatefulTransactionValidatorConfig {
            validate_resource_bounds: true,
            min_gas_price_percentage,
            ..Default::default()
        },
        chain_info: ChainInfo::create_for_testing(),
        state_reader_and_contract_manager: None,
        gateway_fixed_block_state_reader: Box::new(mock_gateway_fixed_block),
    };

    let result = stateful_validator.validate_resource_bounds(&executable_tx).await;
    assert_eq!(result, expected_result);
}

#[rstest]
#[case::nonce_equal_to_account_nonce(0, nonce!(1), nonce!(1), Ok(false))] // Nonce is equal to account nonce.
#[case::nonce_in_allowed_range(10, nonce!(1), nonce!(11), Ok(false))]
#[case::nonce_beyond_allowed_gap(
    10,
    nonce!(1),
    nonce!(12),
    Err(StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::InvalidTransactionNonce))
)]
#[case::nonce_less_then_account_nonce(
    0,
    nonce!(1),
    nonce!(0),
    Err(StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::InvalidTransactionNonce))
)]
#[tokio::test]
async fn test_is_valid_nonce(
    #[case] max_allowed_nonce_gap: u32,
    #[case] account_nonce: Nonce,
    #[case] tx_nonce: Nonce,
    #[case] expected_result: Result<bool, StarknetErrorCode>,
) {
    let executable_tx = executable_invoke_tx(invoke_tx_args!(
        nonce: tx_nonce,
        resource_bounds: ValidResourceBounds::create_for_testing(),
    ));
    run_pre_validation_checks_test(
        executable_tx,
        account_nonce,
        max_allowed_nonce_gap,
        expected_result,
    )
    .await;
}

#[rstest]
#[case::nonce_equal_to_account_nonce(0, Ok(false))]
#[case::nonce_greater_then_account_nonce(
    1,
    Err(StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::InvalidTransactionNonce))
)]
#[case::nonce_less_then_account_nonce(-1, Err(StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::InvalidTransactionNonce)))]
#[tokio::test]
async fn test_reject_future_declares(
    #[case] account_nonce_diff: i32,
    #[case] expected_result: Result<bool, StarknetErrorCode>,
) {
    let account_nonce = 10;

    let executable_tx = executable_declare_tx(
        declare_tx_args!(nonce: nonce!(account_nonce + account_nonce_diff)),
        calculate_class_info_for_testing(
            FeatureContract::Empty(CairoVersion::Cairo1(RunnableCairo1::Casm)).get_class(),
        ),
    );
    run_pre_validation_checks_test(executable_tx, nonce!(account_nonce), 0, expected_result).await;
}

#[rstest]
#[case::all_nonces_zero(0, 0, Ok(false))]
#[case::tx_nonce_nonzero(
    0,
    1,
    Err(StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::InvalidTransactionNonce))
)]
#[case::account_nonce_nonzero(
    1,
    0,
    Err(StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::InvalidTransactionNonce))
)]
#[tokio::test]
async fn test_deploy_account_nonce_validation(
    #[case] account_nonce: u32,
    #[case] tx_nonce: u32,
    #[case] expected_result: Result<bool, StarknetErrorCode>,
) {
    let executable_tx = executable_deploy_account_tx(deploy_account_tx_args!(
        nonce: nonce!(tx_nonce),
        resource_bounds: ValidResourceBounds::create_for_testing(),
    ));

    run_pre_validation_checks_test(executable_tx, nonce!(account_nonce), 0, expected_result).await;
}

async fn run_pre_validation_checks_test(
    executable_tx: AccountTransaction,
    account_nonce: Nonce,
    max_allowed_nonce_gap: u32,
    expected_result: Result<bool, StarknetErrorCode>,
) {
    let mut mock_gateway_fixed_block = MockGatewayFixedBlockStateReader::new();
    mock_gateway_fixed_block
        .expect_get_nonce()
        .with(eq(executable_tx.sender_address()))
        .return_once(move |_| Ok(account_nonce));
    let stateful_validator = StatefulTransactionValidator {
        config: StatefulTransactionValidatorConfig {
            max_allowed_nonce_gap,
            validate_resource_bounds: false,
            ..Default::default()
        },
        chain_info: ChainInfo::create_for_testing(),
        state_reader_and_contract_manager: None,
        gateway_fixed_block_state_reader: Box::new(mock_gateway_fixed_block),
    };

    let mut mempool_client = MockMempoolClient::new();
    mempool_client.expect_validate_tx().returning(|_| Ok(()));
    let result = stateful_validator
        .run_pre_validation_checks(&executable_tx, account_nonce, Arc::new(mempool_client))
        .await
        .map_err(|err| err.code);
    assert_eq!(result, expected_result);
}
