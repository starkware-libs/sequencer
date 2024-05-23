use blockifier::blockifier::stateful_validator::StatefulValidatorError;
use blockifier::context::BlockContext;
use blockifier::test_utils::contracts::FeatureContract;
use blockifier::test_utils::initial_test_state::test_state_reader;
use blockifier::test_utils::{create_trivial_calldata, CairoVersion, NonceManager};
use blockifier::transaction::errors::{TransactionFeeError, TransactionPreValidationError};
use rstest::rstest;
use starknet_api::hash::StarkFelt;
use starknet_api::transaction::TransactionHash;

use crate::config::StatefulTransactionValidatorConfig;
use crate::errors::{StatefulTransactionValidatorError, StatefulTransactionValidatorResult};
use crate::invoke_tx_args;
use crate::starknet_api_test_utils::{
    executable_resource_bounds_mapping, external_invoke_tx, VALID_L1_GAS_MAX_AMOUNT,
    VALID_L1_GAS_MAX_PRICE_PER_UNIT,
};
use crate::state_reader_test_utils::{TestStateReader, TestStateReaderFactory};
use crate::stateful_transaction_validator::StatefulTransactionValidator;

#[rstest]
#[case::valid_invoke_tx(
    100000000000000000,
    Ok(TransactionHash(StarkFelt::try_from(
        "0x07459d76bd7adec02c25cf7ab0dcb95e9197101d4ada41cae6b465fcb78c0e47"
    ).unwrap()))
)]
#[case::invalid_invoke_tx(
    0,
    Err(StatefulTransactionValidatorError::StatefulValidatorError(
        StatefulValidatorError::TransactionPreValidationError(
            TransactionPreValidationError::TransactionFeeError(
                TransactionFeeError::L1GasBoundsExceedBalance {
                    max_amount: VALID_L1_GAS_MAX_AMOUNT,
                    max_price: VALID_L1_GAS_MAX_PRICE_PER_UNIT,
                    balance_low: StarkFelt::ZERO,
                    balance_high: StarkFelt::ZERO,
                }
            )
        )
    ))
)]
fn test_stateful_transaction_validator(
    #[case] account_balance: u128,
    #[case] expected_result: StatefulTransactionValidatorResult<TransactionHash>,
) {
    let cairo_version = CairoVersion::Cairo1;
    let block_context = &BlockContext::create_for_testing();
    let account_contract = FeatureContract::AccountWithoutValidations(cairo_version);
    let contract_address = account_contract.get_instance_address(0);
    let test_contract = FeatureContract::TestContract(cairo_version);
    let test_contract_address = test_contract.get_instance_address(0);

    let state_reader = test_state_reader(
        block_context.chain_info(),
        account_balance,
        &[(account_contract, 1), (test_contract, 1)],
    );

    let state_reader_factory = TestStateReaderFactory {
        state_reader: TestStateReader {
            block_info: block_context.block_info().clone(),
            blockifier_state_reader: state_reader,
        },
    };

    let stateful_validator = StatefulTransactionValidator {
        config: StatefulTransactionValidatorConfig {
            max_nonce_for_validation_skip: Default::default(),
            validate_max_n_steps: block_context.versioned_constants().validate_max_n_steps,
            max_recursion_depth: block_context.versioned_constants().max_recursion_depth,
            chain_info: block_context.chain_info().clone().into(),
        },
    };

    let calldata = create_trivial_calldata(test_contract_address);
    let mut nonce_manager = NonceManager::default();
    let nonce = nonce_manager.next(contract_address);
    let external_tx = external_invoke_tx(invoke_tx_args!(
        resource_bounds: executable_resource_bounds_mapping(),
        nonce,
        contract_address,
        calldata));

    let result = stateful_validator.run_validate(&state_reader_factory, &external_tx, None, None);
    assert_eq!(format!("{:?}", result), format!("{:?}", expected_result));
}
