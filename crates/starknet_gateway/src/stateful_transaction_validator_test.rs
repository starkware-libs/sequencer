use std::sync::Arc;

use blockifier::blockifier::stateful_validator::{
    StatefulValidatorError as BlockifierStatefulValidatorError,
    StatefulValidatorResult as BlockifierStatefulValidatorResult,
};
use blockifier::context::ChainInfo;
use blockifier::transaction::errors::{TransactionFeeError, TransactionPreValidationError};
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use mempool_test_utils::starknet_api_test_utils::{
    executable_invoke_tx as create_executable_invoke_tx,
    VALID_L1_GAS_MAX_AMOUNT,
    VALID_L1_GAS_MAX_PRICE_PER_UNIT,
};
use mockall::predicate::eq;
use num_bigint::BigUint;
use pretty_assertions::assert_eq;
use rstest::{fixture, rstest};
use starknet_api::block::GasPrice;
use starknet_api::core::Nonce;
use starknet_api::executable_transaction::AccountTransaction;
use starknet_api::execution_resources::GasAmount;
use starknet_api::test_utils::deploy_account::executable_deploy_account_tx;
use starknet_api::test_utils::invoke::executable_invoke_tx;
use starknet_api::transaction::fields::Resource;
use starknet_api::{deploy_account_tx_args, invoke_tx_args, nonce};
use starknet_gateway_types::errors::GatewaySpecError;
use starknet_mempool_types::communication::MockMempoolClient;

use crate::config::StatefulTransactionValidatorConfig;
use crate::state_reader::{MockStateReaderFactory, StateReaderFactory};
use crate::state_reader_test_utils::local_test_state_reader_factory;
use crate::stateful_transaction_validator::{
    MockStatefulTransactionValidatorTrait,
    StatefulTransactionValidator,
};

pub const STATEFUL_VALIDATOR_FEE_ERROR: BlockifierStatefulValidatorError =
    BlockifierStatefulValidatorError::TransactionPreValidationError(
        TransactionPreValidationError::TransactionFeeError(
            TransactionFeeError::GasBoundsExceedBalance {
                resource: Resource::L1DataGas,
                max_amount: GasAmount(VALID_L1_GAS_MAX_AMOUNT),
                max_price: GasPrice(VALID_L1_GAS_MAX_PRICE_PER_UNIT),
                balance: BigUint::ZERO,
            },
        ),
    );

#[fixture]
fn stateful_validator() -> StatefulTransactionValidator {
    StatefulTransactionValidator { config: StatefulTransactionValidatorConfig::default() }
}

// TODO(Arni): consider testing declare and deploy account.
#[rstest]
#[case::valid_tx(
    create_executable_invoke_tx(CairoVersion::Cairo1(RunnableCairo1::Casm)),
    Ok(())
)]
#[case::invalid_tx(
    create_executable_invoke_tx(CairoVersion::Cairo1(RunnableCairo1::Casm)),
    Err(STATEFUL_VALIDATOR_FEE_ERROR)
)]
#[tokio::test]
async fn test_stateful_tx_validator(
    #[case] executable_tx: AccountTransaction,
    #[case] expected_result: BlockifierStatefulValidatorResult<()>,
    stateful_validator: StatefulTransactionValidator,
) {
    let expected_result_as_stateful_transaction_result = expected_result
        .as_ref()
        .map(|validate_result| *validate_result)
        .map_err(|blockifier_error| GatewaySpecError::ValidationFailure {
            data: blockifier_error.to_string(),
        });

    let mut mock_validator = MockStatefulTransactionValidatorTrait::new();
    mock_validator.expect_validate().return_once(|_| expected_result.map(|_| ()));

    let account_nonce = nonce!(0);
    let mut mock_mempool_client = MockMempoolClient::new();
    mock_mempool_client.expect_account_tx_in_pool_or_recent_block().returning(|_| {
        // The mempool does not have any transactions from the sender.
        Ok(false)
    });
    let mempool_client = Arc::new(mock_mempool_client);
    let runtime = tokio::runtime::Handle::current();

    tokio::task::spawn_blocking(move || {
        let result = stateful_validator.run_validate(
            &executable_tx,
            account_nonce,
            mempool_client,
            mock_validator,
            runtime,
        );
        assert_eq!(result, expected_result_as_stateful_transaction_result);
    })
    .await
    .unwrap();
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
    true
)]
#[case::should_not_skip_validation_nonce_zero(
    executable_invoke_tx(invoke_tx_args!(nonce: nonce!(0))),
    nonce!(0),
    true,
    false
)]
#[case::should_not_skip_validation_nonce_over_one(
    executable_invoke_tx(invoke_tx_args!(nonce: nonce!(2))),
    nonce!(0),
    true,
    false
)]
// TODO(Arni): Fix this test case. Ideally, we would have a non-invoke transaction with tx_nonce 1
// and account_nonce 0. For deploy account the tx_nonce is always 0. Replace with a declare tx.
#[case::should_not_skip_validation_non_invoke(
    executable_deploy_account_tx(deploy_account_tx_args!()),
    nonce!(0),
    true,
    false

)]
#[case::should_not_skip_validation_account_nonce_1(
    executable_invoke_tx(invoke_tx_args!(nonce: nonce!(1))),
    nonce!(1),
    true,
    false
)]
#[case::should_not_skip_validation_no_tx_in_mempool(
    executable_invoke_tx(invoke_tx_args!(nonce: nonce!(1))),
    nonce!(0),
    false,
    false
)]
#[tokio::test]
async fn test_skip_stateful_validation(
    #[case] executable_tx: AccountTransaction,
    #[case] sender_nonce: Nonce,
    #[case] contains_tx: bool,
    #[case] should_skip_validate: bool,
    stateful_validator: StatefulTransactionValidator,
) {
    let mut mock_validator = MockStatefulTransactionValidatorTrait::new();
    mock_validator
        .expect_validate()
        .withf(move |tx| !tx.execution_flags.validate == should_skip_validate)
        .returning(|_| Ok(()));
    let mut mock_mempool_client = MockMempoolClient::new();
    mock_mempool_client
        .expect_account_tx_in_pool_or_recent_block()
        .returning(move |_| Ok(contains_tx));
    let mempool_client = Arc::new(mock_mempool_client);
    let runtime = tokio::runtime::Handle::current();

    tokio::task::spawn_blocking(move || {
        let _ = stateful_validator.run_validate(
            &executable_tx,
            sender_nonce,
            mempool_client,
            mock_validator,
            runtime,
        );
    })
    .await
    .unwrap();
}
