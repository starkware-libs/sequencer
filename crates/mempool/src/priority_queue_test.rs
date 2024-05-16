use starknet_api::hash::StarkFelt;
use starknet_api::{
    data_availability::DataAvailabilityMode,
    internal_transaction::{InternalInvokeTransaction, InternalTransaction},
    transaction::{
        InvokeTransaction, InvokeTransactionV3, ResourceBounds, ResourceBoundsMapping, Tip,
        TransactionHash,
    },
};

use crate::priority_queue::PriorityQueue;

pub fn create_tx_for_testing(tip: Tip, tx_hash: TransactionHash) -> InternalTransaction {
    let tx = InvokeTransactionV3 {
        resource_bounds: ResourceBoundsMapping::try_from(vec![
            (
                starknet_api::transaction::Resource::L1Gas,
                ResourceBounds::default(),
            ),
            (
                starknet_api::transaction::Resource::L2Gas,
                ResourceBounds::default(),
            ),
        ])
        .expect("Resource bounds mapping has unexpected structure."),
        signature: Default::default(),
        nonce: Default::default(),
        sender_address: Default::default(),
        calldata: Default::default(),
        nonce_data_availability_mode: DataAvailabilityMode::L1,
        fee_data_availability_mode: DataAvailabilityMode::L1,
        paymaster_data: Default::default(),
        account_deployment_data: Default::default(),
        tip,
    };

    InternalTransaction::Invoke(InternalInvokeTransaction {
        tx: InvokeTransaction::V3(tx),
        tx_hash,
        only_query: false,
    })
}

#[tokio::test]
async fn test_priority_queue() {
    let tx_hash_50 = TransactionHash(StarkFelt::ONE);
    let tx_hash_100 = TransactionHash(StarkFelt::TWO);
    let tx_hash_10 = TransactionHash(StarkFelt::THREE);

    let tx_tip_50 = create_tx_for_testing(Tip(50), tx_hash_50);
    let tx_tip_100 = create_tx_for_testing(Tip(100), tx_hash_100);
    let tx_tip_10 = create_tx_for_testing(Tip(10), tx_hash_10);

    let mut pq = PriorityQueue::default();
    pq.push(tx_tip_50.clone());
    pq.push(tx_tip_100.clone());
    pq.push(tx_tip_10.clone());

    assert_eq!(pq.pop().unwrap(), tx_tip_100);
    assert_eq!(pq.pop().unwrap(), tx_tip_50);
    assert_eq!(pq.pop().unwrap(), tx_tip_10);
}
