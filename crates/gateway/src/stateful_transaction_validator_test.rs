use blockifier::blockifier::stateful_validator::{
    StatefulValidatorError as BlockifierStatefulValidatorError,
    StatefulValidatorResult as BlockifierStatefulValidatorResult,
};
use blockifier::context::ChainInfo;
use blockifier::test_utils::CairoVersion;
use blockifier::transaction::errors::{TransactionFeeError, TransactionPreValidationError};
use mempool_test_utils::starknet_api_test_utils::{
    executable_invoke_tx as create_executable_invoke_tx,
    TEST_SENDER_ADDRESS,
    VALID_L1_GAS_MAX_AMOUNT,
    VALID_L1_GAS_MAX_PRICE_PER_UNIT,
};
use mockall::predicate::eq;
use num_bigint::BigUint;
use pretty_assertions::assert_eq;
use rstest::{fixture, rstest};
use starknet_api::core::Nonce;
use starknet_api::executable_transaction::Transaction;
use starknet_api::test_utils::deploy_account::executable_deploy_account_tx;
use starknet_api::test_utils::invoke::executable_invoke_tx;
use starknet_api::transaction::Resource;
use starknet_api::{deploy_account_tx_args, invoke_tx_args, nonce};
use starknet_gateway_types::errors::GatewaySpecError;

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
                max_amount: VALID_L1_GAS_MAX_AMOUNT,
                max_price: VALID_L1_GAS_MAX_PRICE_PER_UNIT,
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
    create_executable_invoke_tx(CairoVersion::Cairo1),
    Ok(())
)]
#[case::invalid_tx(
    create_executable_invoke_tx(CairoVersion::Cairo1),
    Err(STATEFUL_VALIDATOR_FEE_ERROR)
)]
fn test_stateful_tx_validator(
    #[case] executable_tx: Transaction,
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
    mock_validator.expect_validate().return_once(|_, _| expected_result.map(|_| ()));

    let account_nonce = nonce!(0);
    let result = stateful_validator.run_validate(&executable_tx, account_nonce, mock_validator);
    assert_eq!(result, expected_result_as_stateful_transaction_result);
}

#[rstest]
fn test_instantiate_validator(stateful_validator: StatefulTransactionValidator) {
    let state_reader_factory = local_test_state_reader_factory(CairoVersion::Cairo1, false);

    let mut mock_state_reader_factory = MockStateReaderFactory::new();

    // Make sure stateful_validator uses the latest block in the initiall call.
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
    Transaction::Invoke(executable_invoke_tx(invoke_tx_args!(nonce: nonce!(1)))),
    nonce!(0),
    true
)]
#[case::should_not_skip_validation_nonce_over_max_nonce_for_skip(
    Transaction::Invoke(executable_invoke_tx(invoke_tx_args!(nonce: nonce!(0)))),
    nonce!(0),
    false
)]
#[case::should_not_skip_validation_non_invoke(
    Transaction::DeployAccount(
        executable_deploy_account_tx(deploy_account_tx_args!(), nonce!(0))
    ),
    nonce!(0),
    false)]
#[case::should_not_skip_validation_account_nonce_1(
    Transaction::Invoke(executable_invoke_tx(
        invoke_tx_args!(
            nonce: nonce!(1),
            sender_address: TEST_SENDER_ADDRESS.into()
        )
    )),
    nonce!(1),
    false
)]
fn test_skip_stateful_validation(
    #[case] executable_tx: Transaction,
    #[case] sender_nonce: Nonce,
    #[case] should_skip_validate: bool,
    stateful_validator: StatefulTransactionValidator,
) {
    let mut mock_validator = MockStatefulTransactionValidatorTrait::new();
    mock_validator
        .expect_validate()
        .withf(move |_, skip_validate| *skip_validate == should_skip_validate)
        .returning(|_, _| Ok(()));
    let _ = stateful_validator.run_validate(&executable_tx, sender_nonce, mock_validator);
}
