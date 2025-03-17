use futures::channel::mpsc::Receiver;
use futures::stream::StreamExt;
use futures::FutureExt;
use mockall::predicate;
use papyrus_network::network_manager::test_utils::{
    mock_register_broadcast_topic,
    BroadcastNetworkMock,
    MockMessagesToBroadcastReceiver,
    TestSubscriberChannels,
};
use papyrus_network::network_manager::{BroadcastTopicChannels, BroadcastTopicClient};
use papyrus_network_types::network_types::BroadcastedMessageMetadata;
use papyrus_protobuf::mempool::RpcTransactionBatch;
use papyrus_test_utils::{get_rng, GetTestInstance};
use starknet_api::rpc_transaction::{InternalRpcTransaction, RpcTransaction};
use starknet_class_manager_types::transaction_converter::{
    MockTransactionConverterTrait,
    TransactionConverterTrait,
};
use starknet_mempool_p2p_types::communication::MempoolP2pPropagatorRequest;
use starknet_sequencer_infra::component_definitions::ComponentRequestHandler;
use tokio::time::timeout;

use super::MempoolP2pPropagator;

const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);
const MAX_TRANSACTION_BATCH_SIZE: usize = 5;

fn setup() -> (
    BroadcastTopicClient<RpcTransactionBatch>,
    MockMessagesToBroadcastReceiver<RpcTransactionBatch>,
    Receiver<BroadcastedMessageMetadata>,
) {
    let TestSubscriberChannels { mock_network, subscriber_channels } =
        mock_register_broadcast_topic().expect("Failed to create mock network");
    let BroadcastTopicChannels { broadcasted_messages_receiver: _, broadcast_topic_client } =
        subscriber_channels;
    let BroadcastNetworkMock {
        messages_to_broadcast_receiver, continue_propagation_receiver, ..
    } = mock_network;
    (broadcast_topic_client, messages_to_broadcast_receiver, continue_propagation_receiver)
}

fn mock_transaction_conversions(
    num_transactions: usize,
) -> (
    Box<dyn TransactionConverterTrait + Send + 'static>,
    Vec<RpcTransaction>,
    Vec<InternalRpcTransaction>,
) {
    let mut seq = mockall::Sequence::new();
    let mut transaction_converter = MockTransactionConverterTrait::new();
    let mut rpc_transactions = vec![];
    let mut internal_transactions = vec![];
    for _ in 0..num_transactions {
        let internal_tx = InternalRpcTransaction::get_test_instance(&mut get_rng());
        let rpc_transaction = RpcTransaction::get_test_instance(&mut get_rng());
        rpc_transactions.push(rpc_transaction.clone());
        internal_transactions.push(internal_tx.clone());
        transaction_converter
            .expect_convert_internal_rpc_tx_to_rpc_tx()
            .with(predicate::eq(internal_tx))
            .times(1)
            .in_sequence(&mut seq)
            .return_once(move |_| Ok(rpc_transaction));
    }
    (Box::new(transaction_converter), rpc_transactions, internal_transactions)
}

#[tokio::test]
async fn process_handle_add_tx() {
    let (broadcast_topic_client, mut messages_to_broadcast_receiver, _) = setup();
    let (transaction_converter, rpc_transactions, internal_transactions) =
        mock_transaction_conversions(1);
    let rpc_transaction = rpc_transactions.first().unwrap().to_owned();
    let internal_tx = internal_transactions.first().unwrap().to_owned();
    let mut mempool_p2p_propagator =
        MempoolP2pPropagator::new(broadcast_topic_client, transaction_converter, 1);
    mempool_p2p_propagator
        .handle_request(MempoolP2pPropagatorRequest::AddTransaction(internal_tx))
        .await;
    let message = timeout(TIMEOUT, messages_to_broadcast_receiver.next()).await.unwrap().unwrap();
    assert_eq!(message, RpcTransactionBatch(vec![rpc_transaction]));
}

#[tokio::test]
async fn process_handle_continue_propagation() {
    let (broadcast_topic_client, _, mut continue_propagation_receiver) = setup();
    let propagation_metadata = BroadcastedMessageMetadata::get_test_instance(&mut get_rng());
    let transaction_converter = MockTransactionConverterTrait::new();
    let mut mempool_p2p_propagator =
        MempoolP2pPropagator::new(broadcast_topic_client, Box::new(transaction_converter), 1);
    mempool_p2p_propagator
        .handle_request(MempoolP2pPropagatorRequest::ContinuePropagation(
            propagation_metadata.clone(),
        ))
        .await;
    let message = timeout(TIMEOUT, continue_propagation_receiver.next()).await.unwrap().unwrap();
    assert_eq!(message, propagation_metadata);
}

#[tokio::test]
async fn transaction_batch_broadcasted_on_max_size() {
    let (broadcast_topic_client, mut messages_to_broadcast_receiver, _) = setup();

    let (transaction_converter, rpc_transactions, mut internal_transactions) =
        mock_transaction_conversions(MAX_TRANSACTION_BATCH_SIZE);

    let mut mempool_p2p_propagator = MempoolP2pPropagator::new(
        broadcast_topic_client,
        transaction_converter,
        MAX_TRANSACTION_BATCH_SIZE,
    );

    // Assert the first (MAX_TRANSACTION_BATCH_SIZE - 1) transaction do not trigger batch broadcast
    let final_internal_tx = internal_transactions.pop().unwrap();

    for internal_tx in internal_transactions {
        mempool_p2p_propagator
            .handle_request(MempoolP2pPropagatorRequest::AddTransaction(internal_tx))
            .await;
    }

    assert!(messages_to_broadcast_receiver.next().now_or_never().is_none());

    mempool_p2p_propagator
        .handle_request(MempoolP2pPropagatorRequest::AddTransaction(final_internal_tx))
        .await;

    // Assert the MAX_TRANSACTION_BATCH_SIZE message does trigger batch broadcast
    let message = timeout(TIMEOUT, messages_to_broadcast_receiver.next()).await.unwrap().unwrap();
    assert_eq!(message, RpcTransactionBatch(rpc_transactions));
}

#[tokio::test]
async fn transaction_batch_broadcasted_on_request() {
    let (broadcast_topic_client, mut messages_to_broadcast_receiver, _) = setup();

    let (transaction_converter, rpc_transactions, internal_transactions) =
        mock_transaction_conversions(MAX_TRANSACTION_BATCH_SIZE - 1);

    let mut mempool_p2p_propagator = MempoolP2pPropagator::new(
        broadcast_topic_client,
        transaction_converter,
        MAX_TRANSACTION_BATCH_SIZE,
    );

    for internal_tx in internal_transactions {
        mempool_p2p_propagator
            .handle_request(MempoolP2pPropagatorRequest::AddTransaction(internal_tx))
            .await;
    }

    // Assert adding the transaction does not trigger batch broadcast
    assert!(messages_to_broadcast_receiver.next().now_or_never().is_none());

    mempool_p2p_propagator
        .handle_request(MempoolP2pPropagatorRequest::BroadcastQueuedTransactions())
        .await;

    // Assert the request triggered batch broadcast
    let message = timeout(TIMEOUT, messages_to_broadcast_receiver.next()).await.unwrap().unwrap();
    assert_eq!(message, RpcTransactionBatch(rpc_transactions));
}
