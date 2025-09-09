use std::collections::HashMap;
use std::sync::Arc;

use apollo_starknet_client::reader::StateDiff;
use assert_matches::assert_matches;
use starknet_api::block::{BlockNumber, BlockTimestamp, GasPricePerToken};
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::data_availability::L1DataAvailabilityMode;
use starknet_api::rpc_transaction::{
    InternalRpcTransaction,
    InternalRpcTransactionWithoutTxHash,
    RpcInvokeTransaction,
    RpcInvokeTransactionV3,
};
use starknet_api::transaction::fields::Fee;
use starknet_api::transaction::{TransactionHash, TransactionOffsetInBlock};
use starknet_api::tx_hash;
use tokio::task;

use crate::cende_client_types::{
    CendeBlockMetadata,
    CendePreconfirmedTransaction,
    ExecutionResources,
    StarknetClientTransactionReceipt,
    TransactionExecutionStatus,
};
use crate::pre_confirmed_block_writer::{
    PreconfirmedBlockWriterConfig,
    PreconfirmedBlockWriterFactory,
    PreconfirmedBlockWriterFactoryTrait,
};
use crate::pre_confirmed_cende_client::MockPreconfirmedCendeClientTrait;

// Test constants
const TEST_L1_GAS_PRICE: u128 = 100;
const TEST_L1_DATA_GAS_PRICE: u128 = 200;
const TEST_L2_GAS_PRICE: u128 = 50;
const TEST_TIMESTAMP: u64 = 1234567890;
const TEST_ACTUAL_FEE: u128 = 500;
const TEST_BLOCK_NUMBER: u64 = 1;
const TEST_ROUND: u32 = 0;

fn create_test_block_metadata() -> CendeBlockMetadata {
    CendeBlockMetadata {
        status: "PRE_CONFIRMED",
        starknet_version: Default::default(),
        l1_da_mode: L1DataAvailabilityMode::Calldata,
        l1_gas_price: GasPricePerToken {
            price_in_fri: TEST_L1_GAS_PRICE.into(),
            price_in_wei: TEST_L1_GAS_PRICE.into(),
        },
        l1_data_gas_price: GasPricePerToken {
            price_in_fri: TEST_L1_DATA_GAS_PRICE.into(),
            price_in_wei: TEST_L1_DATA_GAS_PRICE.into(),
        },
        l2_gas_price: GasPricePerToken {
            price_in_fri: TEST_L2_GAS_PRICE.into(),
            price_in_wei: TEST_L2_GAS_PRICE.into(),
        },
        timestamp: BlockTimestamp(TEST_TIMESTAMP),
        sequencer_address: ContractAddress::default(),
    }
}

fn create_test_internal_consensus_tx(tx_hash: TransactionHash) -> InternalConsensusTransaction {
    let rpc_invoke_tx_v3 = RpcInvokeTransactionV3 {
        sender_address: ContractAddress::default(),
        calldata: Default::default(),
        signature: Default::default(),
        nonce: Nonce::default(),
        resource_bounds: Default::default(),
        tip: Default::default(),
        paymaster_data: Default::default(),
        account_deployment_data: Default::default(),
        nonce_data_availability_mode: Default::default(),
        fee_data_availability_mode: Default::default(),
    };

    let rpc_invoke_tx = RpcInvokeTransaction::V3(rpc_invoke_tx_v3);
    let internal_rpc_tx_without_hash = InternalRpcTransactionWithoutTxHash::Invoke(rpc_invoke_tx);
    let internal_rpc_tx = InternalRpcTransaction { tx_hash, tx: internal_rpc_tx_without_hash };
    InternalConsensusTransaction::RpcTransaction(internal_rpc_tx)
}

fn create_test_transaction_receipt(tx_hash: TransactionHash) -> StarknetClientTransactionReceipt {
    StarknetClientTransactionReceipt {
        transaction_index: TransactionOffsetInBlock(0),
        transaction_hash: tx_hash,
        l1_to_l2_consumed_message: None,
        l2_to_l1_messages: vec![],
        events: vec![],
        execution_resources: ExecutionResources {
            n_steps: 0,
            builtin_instance_counter: HashMap::new(),
            n_memory_holes: 0,
            data_availability: None,
            total_gas_consumed: None,
        },
        actual_fee: Fee(TEST_ACTUAL_FEE),
        execution_status: TransactionExecutionStatus::Succeeded,
        revert_error: None,
    }
}

fn create_test_state_diff() -> StateDiff {
    StateDiff::default()
}

#[tokio::test]
async fn test_basic_flow_candidate_then_preconfirmed() {
    let tx_hash = tx_hash!(1);
    let internal_consensus_tx = create_test_internal_consensus_tx(tx_hash);
    let receipt = create_test_transaction_receipt(tx_hash);
    let state_diff = create_test_state_diff();

    let mut mock_client = MockPreconfirmedCendeClientTrait::new();

    // Expect first call with empty transactions (write_iteration 0)
    let (iter_0_done_sender, iter_0_done_receiver) = tokio::sync::oneshot::channel::<()>();
    mock_client
        .expect_write_pre_confirmed_block()
        .times(1)
        .withf(|block| {
            block.write_iteration == 0 && block.pre_confirmed_block.transactions.is_empty()
        })
        .return_once(move |_| {
            Box::pin(async move {
                let _ = iter_0_done_sender.send(());
                Ok(())
            })
        });

    // Expect second call with candidate tx but no receipt and state diff (write_iteration 1)
    let (iter_1_done_sender, iter_1_done_receiver) = tokio::sync::oneshot::channel::<()>();
    let expected_candidate_tx = CendePreconfirmedTransaction::from(internal_consensus_tx.clone());
    mock_client
        .expect_write_pre_confirmed_block()
        .times(1)
        .withf(move |block| {
            block.write_iteration == 1
                && block.pre_confirmed_block.transactions.len() == 1
                && block.pre_confirmed_block.transactions[0] == expected_candidate_tx
                && block.pre_confirmed_block.transaction_receipts.len() == 1
                && block.pre_confirmed_block.transaction_receipts[0].is_none()
                && block.pre_confirmed_block.transaction_state_diffs.len() == 1
                && block.pre_confirmed_block.transaction_state_diffs[0].is_none()
        })
        .return_once(move |_| {
            Box::pin(async move {
                let _ = iter_1_done_sender.send(());
                Ok(())
            })
        });

    // Expect third call with pre confirmed tx (write_iteration 2)
    let (iter_2_done_sender, iter_2_done_receiver) = tokio::sync::oneshot::channel::<()>();
    let expected_pre_confirmed_tx =
        CendePreconfirmedTransaction::from(internal_consensus_tx.clone());
    let expected_receipt = Some(receipt.clone());
    let expected_state_diff = Some(state_diff.clone());
    mock_client
        .expect_write_pre_confirmed_block()
        .times(1)
        .withf(move |block| {
            block.write_iteration == 2
                && block.pre_confirmed_block.transactions.len() == 1
                && block.pre_confirmed_block.transactions[0] == expected_pre_confirmed_tx
                && block.pre_confirmed_block.transaction_receipts.len() == 1
                && block.pre_confirmed_block.transaction_receipts[0] == expected_receipt
                && block.pre_confirmed_block.transaction_state_diffs.len() == 1
                && block.pre_confirmed_block.transaction_state_diffs[0] == expected_state_diff
        })
        .return_once(move |_| {
            Box::pin(async move {
                let _ = iter_2_done_sender.send(());
                Ok(())
            })
        });

    let factory = PreconfirmedBlockWriterFactory {
        config: PreconfirmedBlockWriterConfig::default(),
        cende_client: Arc::new(mock_client),
    };

    let (mut writer, candidate_tx_sender, preconfirmed_tx_sender) =
        factory.create(BlockNumber(TEST_BLOCK_NUMBER), TEST_ROUND, create_test_block_metadata());

    let writer_task = task::spawn(async move { writer.run().await });

    // Wait for initial empty write
    iter_0_done_receiver.await.unwrap();

    // Send candidate transaction
    candidate_tx_sender.send(vec![internal_consensus_tx.clone()]).await.unwrap();
    iter_1_done_receiver.await.unwrap();

    // Send preconfirmed transaction
    preconfirmed_tx_sender.send((internal_consensus_tx, receipt, state_diff)).await.unwrap();
    iter_2_done_receiver.await.unwrap();

    // Dropping the senders signals the writer that current block build is complete and it should
    // exit.
    drop(candidate_tx_sender);
    drop(preconfirmed_tx_sender);

    assert_matches!(writer_task.await, Ok(Ok(())));
}

#[tokio::test]
async fn test_preconfirmed_before_candidate_no_extra_write() {
    let tx_hash = tx_hash!(1);
    let internal_consensus_tx = create_test_internal_consensus_tx(tx_hash);
    let receipt = create_test_transaction_receipt(tx_hash);
    let state_diff = create_test_state_diff();

    let mut mock_client = MockPreconfirmedCendeClientTrait::new();

    // Expect first call with empty transactions (write_iteration 0)
    let (iter_0_done_sender, iter_0_done_receiver) = tokio::sync::oneshot::channel::<()>();
    mock_client
        .expect_write_pre_confirmed_block()
        .times(1)
        .withf(|block| {
            block.write_iteration == 0 && block.pre_confirmed_block.transactions.is_empty()
        })
        .return_once(move |_| {
            Box::pin(async move {
                let _ = iter_0_done_sender.send(());
                Ok(())
            })
        });

    // Expect second call with pre confirmed tx (write_iteration 1)
    let (iter_1_done_sender, iter_1_done_receiver) = tokio::sync::oneshot::channel::<()>();
    let expected_pre_confirmed_tx =
        CendePreconfirmedTransaction::from(internal_consensus_tx.clone());
    let expected_receipt = Some(receipt.clone());
    let expected_state_diff = Some(state_diff.clone());
    mock_client
        .expect_write_pre_confirmed_block()
        .times(1)
        .withf(move |block| {
            block.write_iteration == 1
                && block.pre_confirmed_block.transactions.len() == 1
                && block.pre_confirmed_block.transactions[0] == expected_pre_confirmed_tx
                && block.pre_confirmed_block.transaction_receipts.len() == 1
                && block.pre_confirmed_block.transaction_receipts[0] == expected_receipt
                && block.pre_confirmed_block.transaction_state_diffs.len() == 1
                && block.pre_confirmed_block.transaction_state_diffs[0] == expected_state_diff
        })
        .return_once(move |_| {
            Box::pin(async move {
                let _ = iter_1_done_sender.send(());
                Ok(())
            })
        });

    let factory = PreconfirmedBlockWriterFactory {
        config: PreconfirmedBlockWriterConfig::default(),
        cende_client: Arc::new(mock_client),
    };

    let (mut writer, candidate_tx_sender, preconfirmed_tx_sender) =
        factory.create(BlockNumber(TEST_BLOCK_NUMBER), TEST_ROUND, create_test_block_metadata());

    let writer_task = task::spawn(async move { writer.run().await });

    // Wait for initial empty write
    iter_0_done_receiver.await.unwrap();

    // Send preconfirmed transaction first
    preconfirmed_tx_sender
        .send((internal_consensus_tx.clone(), receipt, state_diff))
        .await
        .unwrap();
    iter_1_done_receiver.await.unwrap();

    // Send candidate transaction (should not trigger additional write)
    candidate_tx_sender.send(vec![internal_consensus_tx]).await.unwrap();

    // Dropping the senders signals the writer that current block build is complete and it should
    // exit.
    drop(candidate_tx_sender);
    drop(preconfirmed_tx_sender);

    assert_matches!(writer_task.await, Ok(Ok(())));
}
