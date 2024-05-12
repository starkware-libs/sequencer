use assert_matches::assert_matches;
use rstest::{fixture, rstest};
use starknet_api::core::{ContractAddress, PatriciaKey};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::hash::{StarkFelt, StarkHash};
use starknet_api::internal_transaction::{InternalInvokeTransaction, InternalTransaction};
use starknet_api::transaction::{
    InvokeTransaction, InvokeTransactionV3, ResourceBounds, ResourceBoundsMapping, Tip,
    TransactionHash,
};
use starknet_api::{contract_address, patricia_key};
use starknet_mempool_types::mempool_types::{
    GatewayToMempoolMessage, MempoolNetworkComponent, MempoolToGatewayMessage,
};
use starknet_mempool_types::utils::create_thin_tx_for_testing;
use tokio::sync::mpsc::channel;

use crate::errors::MempoolError;
use crate::mempool::{Account, Mempool, MempoolInput};
use crate::priority_queue::PQTransaction;

fn create_for_testing(inputs: impl IntoIterator<Item = MempoolInput>) -> Mempool {
    let (_, rx_gateway_to_mempool) = channel::<GatewayToMempoolMessage>(1);
    let (tx_mempool_to_gateway, _) = channel::<MempoolToGatewayMessage>(1);
    let network = MempoolNetworkComponent::new(tx_mempool_to_gateway, rx_gateway_to_mempool);

    Mempool::new(inputs, network)
}

#[fixture]
pub fn mempool() -> Mempool {
    create_for_testing([])
}

// TODO(Ayelet): Move to StarkNet API.
pub fn create_internal_invoke_tx_for_testing(
    tip: Tip,
    tx_hash: TransactionHash,
    sender_address: ContractAddress,
) -> InternalTransaction {
    let tx = InvokeTransactionV3 {
        resource_bounds: ResourceBoundsMapping::try_from(vec![
            (starknet_api::transaction::Resource::L1Gas, ResourceBounds::default()),
            (starknet_api::transaction::Resource::L2Gas, ResourceBounds::default()),
        ])
        .unwrap(),
        signature: Default::default(),
        nonce: Default::default(),
        sender_address,
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

#[rstest]
#[case(3)] // Requesting exactly the number of transactions in the queue
#[case(5)] // Requesting more transactions than are in the queue
#[case(2)] // Requesting fewer transactions than are in the queue
fn test_get_txs(#[case] requested_txs: usize) {
    let account1 = Account { address: contract_address!("0x0"), ..Default::default() };
    let tx_tip_50_address_0 =
        create_thin_tx_for_testing(Tip(50), TransactionHash(StarkFelt::ONE), account1.address);
    let account2 = Account { address: contract_address!("0x1"), ..Default::default() };
    let tx_tip_100_address_1 =
        create_thin_tx_for_testing(Tip(100), TransactionHash(StarkFelt::TWO), account2.address);
    let account3 = Account { address: contract_address!("0x2"), ..Default::default() };
    let tx_tip_10_address_2 =
        create_thin_tx_for_testing(Tip(10), TransactionHash(StarkFelt::THREE), account3.address);

    let mut mempool = create_for_testing([
        MempoolInput { tx: tx_tip_50_address_0.clone(), account: account1 },
        MempoolInput { tx: tx_tip_100_address_1.clone(), account: account2 },
        MempoolInput { tx: tx_tip_10_address_2.clone(), account: account3 },
    ]);

    let expected_addresses =
        vec![contract_address!("0x0"), contract_address!("0x1"), contract_address!("0x2")];
    // checks that the transactions were added to the mempool.
    for address in &expected_addresses {
        assert!(mempool.state.contains_key(address));
    }

    let sorted_txs = vec![tx_tip_100_address_1, tx_tip_50_address_0, tx_tip_10_address_2];

    let txs = mempool.get_txs(requested_txs).unwrap();

    // This ensures we do not exceed the priority queue's limit of 3 transactions.
    let max_requested_txs = requested_txs.min(3);

    // checks that the returned transactions are the ones with the highest priority.
    assert_eq!(txs.len(), max_requested_txs);
    assert_eq!(txs, sorted_txs[..max_requested_txs].to_vec());

    // checks that the transactions that were not returned are still in the mempool.
    let actual_addresses: Vec<&ContractAddress> = mempool.state.keys().collect();
    let expected_remaining_addresses: Vec<&ContractAddress> =
        expected_addresses[max_requested_txs..].iter().collect();
    assert_eq!(actual_addresses, expected_remaining_addresses,);
}

#[rstest]
#[should_panic(expected = "Contract address: \
                           ContractAddress(PatriciaKey(StarkFelt(\"\
                           0x0000000000000000000000000000000000000000000000000000000000000000\"\
                           ))) already exists in the mempool. Can't add")]
fn test_mempool_initialization_with_duplicate_contract_addresses() {
    let account = Account { address: contract_address!("0x0"), ..Default::default() };
    let tx = create_thin_tx_for_testing(Tip(50), TransactionHash(StarkFelt::ONE), account.address);
    let same_tx = tx.clone();

    let inputs = vec![MempoolInput { tx, account }, MempoolInput { tx: same_tx, account }];

    // This call should panic because of duplicate contract addresses
    let _mempool = create_for_testing(inputs.into_iter());
}

#[rstest]
fn test_add_tx(mut mempool: Mempool) {
    let account1 = Account::default();
    let tx_tip_50_address_0 =
        create_thin_tx_for_testing(Tip(50), TransactionHash(StarkFelt::ONE), account1.address);
    let account2 = Account { address: contract_address!("0x1"), ..Default::default() };
    let tx_tip_100_address_1 =
        create_thin_tx_for_testing(Tip(100), TransactionHash(StarkFelt::TWO), account2.address);
    let account3 = Account { address: contract_address!("0x2"), ..Default::default() };
    let tx_tip_80_address_2 =
        create_thin_tx_for_testing(Tip(80), TransactionHash(StarkFelt::THREE), account3.address);

    assert!(mempool.add_tx(tx_tip_50_address_0.clone(), account1).is_ok());
    assert!(mempool.add_tx(tx_tip_100_address_1.clone(), account2).is_ok());
    assert!(mempool.add_tx(tx_tip_80_address_2.clone(), account3).is_ok());

    assert_eq!(mempool.state.len(), 3);
    mempool.state.contains_key(&account1.address);
    mempool.state.contains_key(&account2.address);
    mempool.state.contains_key(&account3.address);

    assert_eq!(mempool.txs_queue.pop_last().unwrap(), PQTransaction(tx_tip_100_address_1));
    assert_eq!(mempool.txs_queue.pop_last().unwrap(), PQTransaction(tx_tip_80_address_2));
    assert_eq!(mempool.txs_queue.pop_last().unwrap(), PQTransaction(tx_tip_50_address_0));
}

#[rstest]
fn test_add_same_tx(mut mempool: Mempool) {
    let account = Account::default();
    let tx = create_thin_tx_for_testing(
        Tip(50),
        TransactionHash(StarkFelt::ONE),
        contract_address!("0x0"),
    );
    let same_tx = tx.clone();

    assert!(mempool.add_tx(tx, account).is_ok());
    assert_matches!(
        mempool.add_tx(same_tx, account),
        Err(MempoolError::DuplicateTransaction { tx_hash: TransactionHash(StarkFelt::ONE) })
    );
}
