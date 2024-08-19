use blockifier::blockifier::stateful_validator::{
    StatefulValidatorError as BlockifierStatefulValidatorError,
    StatefulValidatorResult as BlockifierStatefulValidatorResult,
};
use blockifier::context::BlockContext;
use blockifier::test_utils::CairoVersion;
use blockifier::transaction::errors::{TransactionFeeError, TransactionPreValidationError};
use mempool_test_utils::starknet_api_test_utils::{
    executable_invoke_tx,
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
use starknet_types_core::felt::Felt;

use super::ValidateInfo;
use crate::config::StatefulTransactionValidatorConfig;
use crate::errors::GatewaySpecError;
use crate::state_reader::{MockStateReaderFactory, StateReaderFactory};
use crate::state_reader_test_utils::local_test_state_reader_factory;
use crate::stateful_transaction_validator::{
    MockStatefulTransactionValidatorTrait,
    StatefulTransactionValidator,
};

pub const STATEFUL_VALIDATOR_FEE_ERROR: BlockifierStatefulValidatorError =
    BlockifierStatefulValidatorError::TransactionPreValidationError(
        TransactionPreValidationError::TransactionFeeError(
            TransactionFeeError::L1GasBoundsExceedBalance {
                max_amount: VALID_L1_GAS_MAX_AMOUNT,
                max_price: VALID_L1_GAS_MAX_PRICE_PER_UNIT,
                balance: BigUint::ZERO,
            },
        ),
    );

#[fixture]
fn block_context() -> BlockContext {
    BlockContext::create_for_testing()
}

#[fixture]
fn stateful_validator(block_context: BlockContext) -> StatefulTransactionValidator {
    StatefulTransactionValidator {
        config: StatefulTransactionValidatorConfig {
            max_nonce_for_validation_skip: Default::default(),
            validate_max_n_steps: block_context.versioned_constants().validate_max_n_steps,
            max_recursion_depth: block_context.versioned_constants().max_recursion_depth,
            chain_info: block_context.chain_info().clone(),
        },
    }
}

// TODO(Arni): consider testing declare and deploy account.
#[rstest]
#[case::valid_tx(
    executable_invoke_tx(CairoVersion::Cairo1),
    Ok(ValidateInfo{account_nonce: Nonce::default()})
)]
#[case::invalid_tx(executable_invoke_tx(CairoVersion::Cairo1), Err(STATEFUL_VALIDATOR_FEE_ERROR))]
fn test_stateful_tx_validator(
    #[case] executable_tx: Transaction,
    #[case] expected_result: BlockifierStatefulValidatorResult<ValidateInfo>,
    stateful_validator: StatefulTransactionValidator,
) {
    let expected_result_as_stateful_transaction_result =
        expected_result.as_ref().map(|validate_info| *validate_info).map_err(|blockifier_error| {
            GatewaySpecError::ValidationFailure { data: blockifier_error.to_string() }
        });

    let mut mock_validator = MockStatefulTransactionValidatorTrait::new();
    mock_validator.expect_validate().return_once(|_, _| expected_result.map(|_| ()));
    mock_validator.expect_get_nonce().returning(|_| Ok(Nonce(Felt::ZERO)));

    let result = stateful_validator.run_validate(&executable_tx, mock_validator);
    assert_eq!(result, expected_result_as_stateful_transaction_result);
}

#[test]
fn test_instantiate_validator() {
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

    let block_context = &BlockContext::create_for_testing();
    let stateful_validator = StatefulTransactionValidator {
        config: StatefulTransactionValidatorConfig {
            max_nonce_for_validation_skip: Default::default(),
            validate_max_n_steps: block_context.versioned_constants().validate_max_n_steps,
            max_recursion_depth: block_context.versioned_constants().max_recursion_depth,
            chain_info: block_context.chain_info().clone(),
        },
    };
    let blockifier_validator = stateful_validator.instantiate_validator(&mock_state_reader_factory);
    assert!(blockifier_validator.is_ok());
}

#[rstest]
#[case::should_skip_validation(
    Transaction::Invoke(starknet_api::test_utils::invoke::executable_invoke_tx(
        starknet_api::invoke_tx_args!(nonce: Nonce(Felt::ONE))
    )),
    Nonce::default(),
    true
)]
#[case::should_not_skip_validation_nonce_over_max_nonce_for_skip(
    Transaction::Invoke(starknet_api::test_utils::invoke::executable_invoke_tx(
        starknet_api::invoke_tx_args!(nonce: Nonce(Felt::ZERO))
    )),
    Nonce::default(),
    false
)]
#[case::should_not_skip_validation_non_invoke(
    Transaction::DeployAccount(
        starknet_api::test_utils::deploy_account::executable_deploy_account_tx(
            starknet_api::deploy_account_tx_args!(), Nonce::default()
        )
    ),
    Nonce::default(),
    false)]
#[case::should_not_skip_validation_account_nonce_1(
    Transaction::Invoke(starknet_api::test_utils::invoke::executable_invoke_tx(
        starknet_api::invoke_tx_args!(
            nonce: Nonce(Felt::ONE),
            sender_address: TEST_SENDER_ADDRESS.into()
        )
    )),
    Nonce(Felt::ONE),
    false
)]
fn test_skip_stateful_validation(
    #[case] executable_tx: Transaction,
    #[case] sender_nonce: Nonce,
    #[case] should_skip_validate: bool,
    stateful_validator: StatefulTransactionValidator,
) {
    let sender_address = executable_tx.contract_address();

    let mut mock_validator = MockStatefulTransactionValidatorTrait::new();
    mock_validator
        .expect_get_nonce()
        .withf(move |contract_address| *contract_address == sender_address)
        .returning(move |_| Ok(sender_nonce));
    mock_validator
        .expect_validate()
        .withf(move |_, skip_validate| *skip_validate == should_skip_validate)
        .returning(|_, _| Ok(()));
    let _ = stateful_validator.run_validate(&executable_tx, mock_validator);
}
