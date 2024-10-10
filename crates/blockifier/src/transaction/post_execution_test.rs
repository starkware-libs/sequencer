use assert_matches::assert_matches;
use rstest::rstest;
use starknet_api::core::{ContractAddress, PatriciaKey};
use starknet_api::execution_resources::GasAmount;
use starknet_api::state::StorageKey;
use starknet_api::transaction::{
    AllResourceBounds,
    Calldata,
    Fee,
    GasVectorComputationMode,
    Resource,
    ResourceBounds,
    TransactionVersion,
    ValidResourceBounds,
};
use starknet_api::{felt, invoke_tx_args, patricia_key};
use starknet_types_core::felt::Felt;

use crate::context::{BlockContext, ChainInfo};
use crate::fee::fee_checks::FeeCheckError;
use crate::state::state_api::StateReader;
use crate::test_utils::contracts::FeatureContract;
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::{
    create_calldata,
    CairoVersion,
    BALANCE,
    DEFAULT_STRK_L1_DATA_GAS_PRICE,
    DEFAULT_STRK_L1_GAS_PRICE,
    DEFAULT_STRK_L2_GAS_PRICE,
};
use crate::transaction::account_transaction::AccountTransaction;
use crate::transaction::errors::TransactionExecutionError;
use crate::transaction::objects::{FeeType, HasRelatedFeeType, TransactionInfoCreator};
use crate::transaction::test_utils::{
    account_invoke_tx,
    block_context,
    create_all_resource_bounds,
    default_all_resource_bounds,
    default_l1_resource_bounds,
    l1_resource_bounds,
    max_fee,
    run_invoke_tx,
    TestInitData,
};
use crate::transaction::transactions::ExecutableTransaction;

fn init_data_by_version(chain_info: &ChainInfo, cairo_version: CairoVersion) -> TestInitData {
    let test_contract = FeatureContract::TestContract(cairo_version);
    let account_contract = FeatureContract::AccountWithoutValidations(cairo_version);
    let state = test_state(chain_info, BALANCE, &[(account_contract, 1), (test_contract, 1)]);
    TestInitData {
        state,
        account_address: account_contract.get_instance_address(0),
        contract_address: test_contract.get_instance_address(0),
        nonce_manager: Default::default(),
    }
}

fn calldata_for_write_and_transfer(
    test_contract_address: ContractAddress,
    storage_address: Felt,
    storage_value: Felt,
    recipient: Felt,
    transfer_amount: Felt,
    fee_token_address: ContractAddress,
) -> Calldata {
    create_calldata(
        test_contract_address,
        "test_write_and_transfer",
        &[
            storage_address,            // Calldata: storage address.
            storage_value,              // Calldata: storage value.
            recipient,                  // Calldata: to.
            transfer_amount,            // Calldata: amount.
            *fee_token_address.0.key(), // Calldata: fee token address.
        ],
    )
}

/// Tests that when a transaction drains an account's balance before fee transfer, the execution is
/// reverted.
#[rstest]
#[case(TransactionVersion::ONE, FeeType::Eth)]
#[case(TransactionVersion::THREE, FeeType::Strk)]
fn test_revert_on_overdraft(
    max_fee: Fee,
    default_all_resource_bounds: ValidResourceBounds,
    block_context: BlockContext,
    #[case] version: TransactionVersion,
    #[case] fee_type: FeeType,
    #[values(CairoVersion::Cairo0)] cairo_version: CairoVersion,
) {
    let chain_info = &block_context.chain_info;
    let fee_token_address = chain_info.fee_token_addresses.get_by_fee_type(&fee_type);
    // An address to be written into to observe state changes.
    let storage_address = felt!(10_u8);
    let storage_key = StorageKey::try_from(storage_address).unwrap();
    // Final storage value expected in the address at the end of this test.
    let expected_final_value = felt!(77_u8);
    // An address to be used as recipient of a transfer.
    let recipient_int = 7_u8;
    let recipient = felt!(recipient_int);
    let recipient_address = ContractAddress(patricia_key!(recipient_int));
    // Amount expected to be transferred successfully.
    let final_received_amount = felt!(80_u8);

    let TestInitData { mut state, account_address, contract_address, mut nonce_manager } =
        init_data_by_version(chain_info, cairo_version);

    // Verify the contract's storage key initial value is empty.
    assert_eq!(state.get_storage_at(contract_address, storage_key).unwrap(), felt!(0_u8));

    // Approve the test contract to transfer funds.
    let approve_calldata = create_calldata(
        fee_token_address,
        "approve",
        &[
            *contract_address.0.key(), // Calldata: to.
            felt!(BALANCE.0),
            felt!(0_u8),
        ],
    );

    let approve_tx: AccountTransaction = account_invoke_tx(invoke_tx_args! {
        max_fee,
        sender_address: account_address,
        calldata: approve_calldata,
        version,
        resource_bounds: default_all_resource_bounds,
        nonce: nonce_manager.next(account_address),
    });
    let tx_info = approve_tx.create_tx_info();
    let approval_execution_info =
        approve_tx.execute(&mut state, &block_context, true, true).unwrap();
    assert!(!approval_execution_info.is_reverted());

    // Transfer a valid amount of funds to compute the cost of a successful
    // `test_write_and_transfer` operation. This operation should succeed.
    let execution_info = run_invoke_tx(
        &mut state,
        &block_context,
        invoke_tx_args! {
            max_fee,
            sender_address: account_address,
            calldata: calldata_for_write_and_transfer(
                contract_address,
                storage_address,
                expected_final_value,
                recipient,
                final_received_amount,
                fee_token_address
            ),
            version,
            resource_bounds: default_all_resource_bounds,
            nonce: nonce_manager.next(account_address),
        },
    )
    .unwrap();

    assert!(!execution_info.is_reverted());
    let transfer_tx_fee = execution_info.receipt.fee;

    // Check the current balance, before next transaction.
    let (balance, _) = state
        .get_fee_token_balance(account_address, chain_info.fee_token_address(&tx_info.fee_type()))
        .unwrap();

    // Attempt to transfer the entire balance, such that no funds remain to pay transaction fee.
    // This operation should revert.
    let execution_info = run_invoke_tx(
        &mut state,
        &block_context,
        invoke_tx_args! {
            max_fee,
            sender_address: account_address,
            calldata: calldata_for_write_and_transfer(
                contract_address,
                storage_address,
                felt!(0_u8),
                recipient,
                balance,
                fee_token_address
            ),
            version,
            resource_bounds: default_all_resource_bounds,
            nonce: nonce_manager.next(account_address),
        },
    )
    .unwrap();

    // Compute the expected balance after the reverted write+transfer (tx fee should be charged).
    let expected_new_balance = balance - Felt::from(transfer_tx_fee.0);

    // Verify the execution was reverted (including nonce bump) with the correct error.
    assert!(execution_info.is_reverted());
    assert!(execution_info.revert_error.unwrap().starts_with("Insufficient fee token balance"));
    assert_eq!(state.get_nonce_at(account_address).unwrap(), nonce_manager.next(account_address));

    // Verify the storage key/value were not updated in the last tx.
    assert_eq!(state.get_storage_at(contract_address, storage_key).unwrap(), expected_final_value);

    // Verify balances of both sender and recipient are as expected.
    assert_eq!(
        state
            .get_fee_token_balance(
                account_address,
                chain_info.fee_token_address(&tx_info.fee_type()),
            )
            .unwrap(),
        (expected_new_balance, felt!(0_u8))
    );
    assert_eq!(
        state
            .get_fee_token_balance(
                recipient_address,
                chain_info.fee_token_address(&tx_info.fee_type())
            )
            .unwrap(),
        (final_received_amount, felt!(0_u8))
    );
}

/// Tests that when a transaction requires more resources than what the sender bounds allow, the
/// execution is reverted; in the non-revertible case, checks for the correct error.
// TODO(Aner, 21/01/24) modify for 4844 (taking blob_gas into account).
#[rstest]
#[case::v0_no_revert(TransactionVersion::ZERO, false, default_all_resource_bounds(), None)]
#[case::v1_insufficient_max_fee(TransactionVersion::ONE, true, default_all_resource_bounds(), None)]
#[case::l1_bounds_insufficient(
    TransactionVersion::THREE,
    true,
    default_l1_resource_bounds(),
    Some(Resource::L1Gas)
)]
#[case::all_bounds_insufficient_l1_gas(
    TransactionVersion::THREE,
    true,
    default_all_resource_bounds(),
    Some(Resource::L1Gas)
)]
#[case::all_bounds_insufficient_l2_gas(
    TransactionVersion::THREE,
    true,
    default_all_resource_bounds(),
    Some(Resource::L2Gas)
)]
#[case::all_bounds_insufficient_l1_data_gas(
    TransactionVersion::THREE,
    true,
    default_all_resource_bounds(),
    Some(Resource::L1DataGas)
)]
fn test_revert_on_resource_overuse(
    max_fee: Fee,
    mut block_context: BlockContext,
    #[case] version: TransactionVersion,
    #[case] is_revertible: bool,
    #[case] resource_bounds: ValidResourceBounds,
    #[case] resource_to_decrement: Option<Resource>,
    #[values(CairoVersion::Cairo0)] cairo_version: CairoVersion,
) {
    block_context.block_info.use_kzg_da = true;
    let gas_mode = resource_bounds.get_gas_vector_computation_mode();
    let fee_type = if version == TransactionVersion::THREE { FeeType::Strk } else { FeeType::Eth };
    let gas_prices = block_context.block_info.gas_prices.get_gas_prices_by_fee_type(&fee_type);
    let TestInitData { mut state, account_address, contract_address, mut nonce_manager } =
        init_data_by_version(&block_context.chain_info, cairo_version);

    let n_writes = 5_u8;
    let base_args = invoke_tx_args! { sender_address: account_address, version };

    // Utility function to generate calldata for the `write_a_lot` function.
    // Change the written value each call to keep cost high.
    let mut value_to_write = 1_u8;
    let mut write_a_lot_calldata = || {
        value_to_write += 1;
        create_calldata(contract_address, "write_a_lot", &[felt!(n_writes), felt!(value_to_write)])
    };

    // Run a "heavy" transaction and measure the resources used.
    // In this context, "heavy" means: a substantial fraction of the cost is not cairo steps.
    // We need this kind of invocation, to be able to test the specific scenario: the resource
    // bounds must be enough to allow completion of the transaction, and yet must still fail
    // post-execution bounds check.
    let execution_info_measure = run_invoke_tx(
        &mut state,
        &block_context,
        invoke_tx_args! {
            max_fee,
            resource_bounds,
            nonce: nonce_manager.next(account_address),
            calldata: write_a_lot_calldata(),
            ..base_args.clone()
        },
    )
    .unwrap();
    assert_eq!(execution_info_measure.revert_error, None);

    let actual_fee = execution_info_measure.receipt.fee;
    let actual_gas_usage = execution_info_measure.receipt.resources.to_gas_vector(
        &block_context.versioned_constants,
        block_context.block_info.use_kzg_da,
        &gas_mode,
    );
    // Final bounds checked depend on the gas mode; in NoL2Gas mode, data gas is converted to L1 gas
    // units for bounds check in post-execution.
    let tight_resource_bounds = match gas_mode {
        GasVectorComputationMode::NoL2Gas => l1_resource_bounds(
            actual_gas_usage.to_discounted_l1_gas(gas_prices),
            DEFAULT_STRK_L1_GAS_PRICE.into(),
        ),
        GasVectorComputationMode::All => {
            ValidResourceBounds::all_bounds_from_vectors(&actual_gas_usage, gas_prices)
        }
    };

    // Run the same function, with a different written value (to keep cost high), with the actual
    // resources used as upper bounds. Make sure execution does not revert.
    let execution_info_tight = run_invoke_tx(
        &mut state,
        &block_context,
        invoke_tx_args! {
            max_fee: actual_fee,
            resource_bounds: tight_resource_bounds,
            nonce: nonce_manager.next(account_address),
            calldata: write_a_lot_calldata(),
            ..base_args.clone()
        },
    )
    .unwrap();
    assert_eq!(execution_info_tight.revert_error, None);
    assert_eq!(execution_info_tight.receipt.fee, actual_fee);
    assert_eq!(execution_info_tight.receipt.resources, execution_info_measure.receipt.resources);

    // Re-run the same function with max bounds slightly below the actual usage, and verify it's
    // reverted.
    let low_max_fee = Fee(execution_info_measure.receipt.fee.0 - 1);
    let low_bounds = if version < TransactionVersion::THREE {
        // Dummy value for deprecated transaction case.
        default_all_resource_bounds()
    } else {
        match tight_resource_bounds {
            ValidResourceBounds::L1Gas(ResourceBounds { max_amount, .. }) => {
                l1_resource_bounds(GasAmount(max_amount.0 - 1), DEFAULT_STRK_L1_GAS_PRICE.into())
            }
            ValidResourceBounds::AllResources(AllResourceBounds {
                l1_gas: ResourceBounds { max_amount: mut l1_gas, .. },
                l2_gas: ResourceBounds { max_amount: mut l2_gas, .. },
                l1_data_gas: ResourceBounds { max_amount: mut l1_data_gas, .. },
            }) => {
                match resource_to_decrement.unwrap() {
                    Resource::L1Gas => l1_gas.0 -= 1,
                    Resource::L2Gas => l2_gas.0 -= 1,
                    Resource::L1DataGas => l1_data_gas.0 -= 1,
                }
                create_all_resource_bounds(
                    l1_gas,
                    DEFAULT_STRK_L1_GAS_PRICE.into(),
                    l2_gas,
                    DEFAULT_STRK_L2_GAS_PRICE.into(),
                    l1_data_gas,
                    DEFAULT_STRK_L1_DATA_GAS_PRICE.into(),
                )
            }
        }
    };
    let execution_info_result = run_invoke_tx(
        &mut state,
        &block_context,
        invoke_tx_args! {
            max_fee: low_max_fee,
            resource_bounds: low_bounds,
            nonce: nonce_manager.next(account_address),
            calldata: write_a_lot_calldata(),
            ..base_args
        },
    );

    // Assert the transaction was reverted with the correct error.
    let expected_error_prefix = if version == TransactionVersion::ZERO {
        ""
    } else if version == TransactionVersion::ONE {
        "Insufficient max fee"
    } else {
        assert_eq!(version, TransactionVersion::THREE);
        &format!("Insufficient max {}", resource_to_decrement.unwrap())
    };
    if is_revertible {
        assert!(
            execution_info_result.unwrap().revert_error.unwrap().starts_with(expected_error_prefix)
        );
    } else {
        assert_matches!(
            execution_info_result.unwrap_err(),
            TransactionExecutionError::FeeCheckError(
                FeeCheckError::MaxFeeExceeded { max_fee, actual_fee: fee_in_error }
            )
            if (max_fee, fee_in_error) == (low_max_fee, actual_fee)
        );
    }
}
