use assert_matches::assert_matches;
use blockifier::blockifier::stateful_validator::StatefulValidatorError;
use blockifier::context::BlockContext;
use blockifier::test_utils::dict_state_reader::DictStateReader;
use blockifier::test_utils::CairoVersion;
use blockifier::transaction::errors::{TransactionFeeError, TransactionPreValidationError};
use mempool_test_utils::invoke_tx_args;
use mempool_test_utils::starknet_api_test_utils::{
    declare_tx, deploy_account_tx, external_invoke_tx, invoke_tx, TEST_SENDER_ADDRESS,
    VALID_L1_GAS_MAX_AMOUNT, VALID_L1_GAS_MAX_PRICE_PER_UNIT,
};
use num_bigint::BigUint;
use pretty_assertions::assert_eq;
use rstest::{fixture, rstest};
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::felt;
use starknet_api::rpc_transaction::RPCTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_types_core::felt::Felt;

use crate::compilation::GatewayCompiler;
use crate::config::StatefulTransactionValidatorConfig;
use crate::errors::{StatefulTransactionValidatorError, StatefulTransactionValidatorResult};
use crate::state_reader_test_utils::{
    local_test_state_reader_factory, local_test_state_reader_factory_for_deploy_account,
    TestStateReader, TestStateReaderFactory,
};
use crate::stateful_transaction_validator::StatefulTransactionValidator;

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
            chain_info: block_context.chain_info().clone().into(),
        },
    }
}

#[rstest]
#[case::valid_invoke_tx_cairo1(
    invoke_tx(CairoVersion::Cairo1),
    local_test_state_reader_factory(CairoVersion::Cairo1, false),
    Ok(TransactionHash(felt!(
        "0x007d70505b4487a4e1c1a4b4e4342cb5aa9e73b86d031891170c45a57ad8b4e6"
    )))
)]
#[case::valid_invoke_tx_cairo0(
    invoke_tx(CairoVersion::Cairo0),
    local_test_state_reader_factory(CairoVersion::Cairo0, false),
    Ok(TransactionHash(felt!(
        "0x032e3a969a64027f15ce2b526d8dff47d47524c58ff0363f93ce4cbe7c280861"
    )))
)]
#[case::valid_deploy_account_tx(
    deploy_account_tx(),
    local_test_state_reader_factory_for_deploy_account(&external_tx),
    Ok(TransactionHash(felt!(
        "0x013287740b37dc112391de4ef0f7cd7aeca323537ca2a78a1108c6aee5a55d70"
    )))
)]
#[case::valid_declare_tx(
    declare_tx(),
    local_test_state_reader_factory(CairoVersion::Cairo1, false),
    Ok(TransactionHash(felt!(
        "0x02da54b89e00d2e201f8e3ed2bcc715a69e89aefdce88aff2d2facb8dec55c0a"
    )))
)]
#[case::invalid_tx(
    invoke_tx(CairoVersion::Cairo1),
    local_test_state_reader_factory(CairoVersion::Cairo1, true),
    Err(StatefulTransactionValidatorError::StatefulValidatorError(
        StatefulValidatorError::TransactionPreValidationError(
            TransactionPreValidationError::TransactionFeeError(
                TransactionFeeError::L1GasBoundsExceedBalance {
                    max_amount: VALID_L1_GAS_MAX_AMOUNT,
                    max_price: VALID_L1_GAS_MAX_PRICE_PER_UNIT,
                    balance: BigUint::ZERO,
                }
            )
        )
    ))
)]
fn test_stateful_tx_validator(
    #[case] external_tx: RPCTransaction,
    #[case] state_reader_factory: TestStateReaderFactory,
    #[case] expected_result: StatefulTransactionValidatorResult<TransactionHash>,
    stateful_validator: StatefulTransactionValidator,
) {
    let optional_class_info = match &external_tx {
        RPCTransaction::Declare(declare_tx) => {
            let gateway_compiler = GatewayCompiler { config: Default::default() };
            Some(gateway_compiler.compile_contract_class(declare_tx).unwrap())
        }
        _ => None,
    };

    let validator = stateful_validator.instantiate_validator(&state_reader_factory).unwrap();

    let result = stateful_validator.run_validate(&external_tx, optional_class_info, validator);
    assert_eq!(format!("{:?}", result), format!("{:?}", expected_result));
}

#[test]
fn test_instantiate_validator() {
    let state_reader_factory = local_test_state_reader_factory(CairoVersion::Cairo1, false);
    let block_context = &BlockContext::create_for_testing();
    let stateful_validator = StatefulTransactionValidator {
        config: StatefulTransactionValidatorConfig {
            max_nonce_for_validation_skip: Default::default(),
            validate_max_n_steps: block_context.versioned_constants().validate_max_n_steps,
            max_recursion_depth: block_context.versioned_constants().max_recursion_depth,
            chain_info: block_context.chain_info().clone().into(),
        },
    };
    let blockifier_validator = stateful_validator.instantiate_validator(&state_reader_factory);
    assert!(blockifier_validator.is_ok());
}

#[rstest]
#[case::should_skip_validation(
    external_invoke_tx(invoke_tx_args!{nonce: Nonce(Felt::ONE)}),
    empty_state_reader_factory(),
    true
)]
#[case::should_not_skip_validation_nonce_over_max_nonce_for_skip(
    external_invoke_tx(invoke_tx_args!{nonce: Nonce(Felt::TWO)}),
    empty_state_reader_factory(),
    false
)]
#[case::should_not_skip_validation_non_invoke(
    deploy_account_tx(),
    empty_state_reader_factory(),
    false
)]
#[case::should_not_skip_validation_account_nonce_1(
    external_invoke_tx(invoke_tx_args!{sender_address: ContractAddress::from(TEST_SENDER_ADDRESS), nonce: Nonce(Felt::ONE)}),
    state_reader_factory_account_nonce_1(ContractAddress::from(TEST_SENDER_ADDRESS)),
    false
)]
// TODO(yael 10/7/2024): use mock validator in this test once ready.
fn test_skip_stateful_validation(
    #[case] external_tx: RPCTransaction,
    #[case] state_reader_factory: TestStateReaderFactory,
    #[case] should_pass_validation: bool,
    stateful_validator: StatefulTransactionValidator,
) {
    let validator = stateful_validator.instantiate_validator(&state_reader_factory).unwrap();
    let result = stateful_validator.run_validate(&external_tx, None, validator);
    if should_pass_validation {
        assert_matches!(result, Ok(_));
    } else {
        // To be sure that the validations were actually skipped, we check that the error came from
        // the blockifier stateful validations, and not from the pre validations since those are
        // executed also when skip_validate is true.
        assert_matches!(result, Err(StatefulTransactionValidatorError::StatefulValidatorError(err)) 
            if !matches!(err, StatefulValidatorError::TransactionPreValidationError(_)));
    }
}

fn empty_state_reader_factory() -> TestStateReaderFactory {
    let block_context = BlockContext::create_for_testing();
    TestStateReaderFactory {
        state_reader: TestStateReader {
            blockifier_state_reader: DictStateReader::default(),
            block_info: block_context.block_info().clone(),
        },
    }
}

fn state_reader_factory_account_nonce_1(sender_address: ContractAddress) -> TestStateReaderFactory {
    let mut state_reader_factory = empty_state_reader_factory();
    state_reader_factory
        .state_reader
        .blockifier_state_reader
        .address_to_nonce
        .insert(sender_address, Nonce(Felt::ONE));
    state_reader_factory
}
