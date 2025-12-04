use std::str::FromStr;

use alloy::consensus::Header as HeaderInner;
use alloy::primitives::{
    keccak256,
    BlockHash,
    Bytes,
    Log as LogInner,
    LogData,
    TxHash,
    B256,
    U256,
};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::{Block, BlockTransactions, Header, Log};
use alloy::transports::mock::Asserter;
use apollo_l1_provider::event_identifiers_to_track;
use papyrus_base_layer::ethereum_base_layer_contract::{
    EthereumBaseLayerConfig,
    EthereumBaseLayerContract,
};
use papyrus_base_layer::test_utils::{
    DEFAULT_ANVIL_L1_ACCOUNT_ADDRESS,
    DEFAULT_ANVIL_L1_DEPLOYED_ADDRESS,
};
mod utils;
use papyrus_base_layer::BaseLayerContract;
use utils::{L1_CONTRACT_ADDRESS, L2_ENTRY_POINT};

// This test requires that we do some manual work to produce the logs we expect to get from the
// Starknet L1 contract. The reason we don't just post events to L1 and have them scraped is that
// some of the log types don't correspond to actions we can just do to the base layer, like marking
// a tx as consumed on L2 (which requires a state update). We also don't know which additional logs
// may be added to the list of filtered logs, which is the point of this test (to protect against
// future additions). So we leave it to anyone that adds that new message from L1 to L2 to also make
// an example log and post it as part of the test, to make sure it is properly parsed all the way up
// to the provider.

const FAKE_HASH: &str = "0x1234567890123456789012345678901234567890123456789012345678901234";

// Each function signature is hashed using keccak257 to get the selector.
// For example, LogMessageToL2(address,uint256,uint256,uint256[],uint256,uint256)
// becomes "db80dd488acf86d17c747445b0eabb5d57c541d3bd7b6b87af987858e5066b2b".
fn filter_to_hash(filter: &str) -> String {
    format!("{:x}", keccak256(filter.as_bytes()))
}

/// Encodes the non-indexed parameters of LogMessageToL2 event data.
/// Parameters: (payload: uint256[], nonce: uint256, fee: uint256)
///
/// ABI encoding for tuple (uint256[], uint256, uint256):
/// - Offset to array (32 bytes)
/// - nonce value (32 bytes)
/// - fee value (32 bytes)
/// - Array length (32 bytes)
/// - Array elements (32 bytes each)
fn encode_log_message_to_l2_data(payload: &[U256], nonce: U256, fee: U256) -> Bytes {
    // Instead of the payload array data, we only put in the head section the offset to where the
    // data will be stored. This would be 3 words from the start of the head (offset, nonce, fee =
    // 96 bytes = 0x60).
    let offset = U256::from(96u64);

    let mut encoded = Vec::new();
    // Offset to the array data (96 bytes).
    encoded.extend_from_slice(&offset.to_be_bytes::<32>());
    // nonce.
    encoded.extend_from_slice(&nonce.to_be_bytes::<32>());
    // fee.
    encoded.extend_from_slice(&fee.to_be_bytes::<32>());
    // Tail section has the payload array data only. It starts with the length of the array.
    let array_len = U256::from(payload.len());
    encoded.extend_from_slice(&array_len.to_be_bytes::<32>());
    // Finally, write the array elements.
    for item in payload {
        encoded.extend_from_slice(&item.to_be_bytes::<32>());
    }

    Bytes::from(encoded)
}

// Same as above, but for the other event types (that don't include a fee).
fn encode_other_event_data(payload: &[U256], nonce: U256) -> Bytes {
    // Instead of the payload array data, we only put in the head section the offset to where the
    // data will be stored. This would be 2 words from the start of the head (offset, nonce = 64
    // bytes = 0x40).
    let offset = U256::from(64u64);

    let mut encoded = Vec::new();
    // Offset to the array data (96 bytes).
    encoded.extend_from_slice(&offset.to_be_bytes::<32>());
    // nonce.
    encoded.extend_from_slice(&nonce.to_be_bytes::<32>());
    // Tail section has the payload array data only. It starts with the length of the array.
    let array_len = U256::from(payload.len());
    encoded.extend_from_slice(&array_len.to_be_bytes::<32>());
    // Finally, write the array elements.
    for item in payload {
        encoded.extend_from_slice(&item.to_be_bytes::<32>());
    }

    Bytes::from(encoded)
}

fn encode_message_into_log(
    selector: &str,
    block_number: u64,
    payload: &[U256],
    nonce: U256,
    fee: Option<U256>,
) -> Log {
    // Add zero padding to the address to make it 32 bytes
    let starknet_address = DEFAULT_ANVIL_L1_ACCOUNT_ADDRESS.to_bigint().to_str_radix(16);
    let starknet_address = format!("{:0>64}", starknet_address);

    let encoded_data = match fee {
        Some(fee) => encode_log_message_to_l2_data(payload, nonce, fee),
        None => encode_other_event_data(payload, nonce),
    };
    Log {
        inner: LogInner {
            address: DEFAULT_ANVIL_L1_DEPLOYED_ADDRESS.parse().unwrap(),
            data: LogData::new_unchecked(
                vec![
                    filter_to_hash(selector).parse().unwrap(),
                    starknet_address.parse().unwrap(),
                    U256::from(L1_CONTRACT_ADDRESS).into(),
                    U256::from(L2_ENTRY_POINT).into(),
                ],
                encoded_data,
            ),
        },
        block_hash: Some(BlockHash::from_str(FAKE_HASH).unwrap()),
        block_number: Some(block_number),
        block_timestamp: None,
        transaction_hash: Some(TxHash::from_str(FAKE_HASH).unwrap()),
        transaction_index: Some(block_number + 1),
        log_index: Some(block_number + 2),
        removed: false,
    }
}

#[tokio::test]
async fn all_event_types_must_be_filtered_and_parsed() {
    // Setup.
    // Make a mock L1
    let asserter = Asserter::new();
    let provider = ProviderBuilder::new().connect_mocked_client(asserter.clone());

    let mut base_layer = EthereumBaseLayerContract::new_with_provider(
        EthereumBaseLayerConfig::default(),
        provider.root().clone(),
    );

    // We can just return the same block all the time, it will only affect the timestamps.
    let dummy_block = Block {
        header: Header {
            hash: BlockHash::from_str(FAKE_HASH).unwrap(),
            inner: HeaderInner { number: 3, base_fee_per_gas: Some(5), ..Default::default() },
            total_difficulty: None,
            size: None,
        },
        transactions: BlockTransactions::<B256>::default(),
        uncles: vec![],
        withdrawals: None,
    };

    // Put together the log that corresponds to each type of event in event_identifiers_to_track().
    // Then filter them one at a time. If any iteration doesn't return an event, it means we fail to
    // filter for it. If any iteration returns an error, we know something is wrong.
    // TODO(guyn): add the scraper and provider parsing.
    let mut block_number = 1;
    let filters = event_identifiers_to_track();

    let mut expected_logs = Vec::with_capacity(filters.len());

    // This log is for LOG_MESSAGE_TO_L2_EVENT_IDENTIFIER (must check that this is the first log in
    // filters)
    let expected_message_to_l2_log = encode_message_into_log(
        filters[0],
        block_number,
        &[U256::from(15), U256::from(202)],
        U256::from(127),
        Some(U256::from(420)),
    );
    block_number += 1;
    asserter.push_success(&vec![expected_message_to_l2_log.clone()]);
    expected_logs.push(expected_message_to_l2_log);
    asserter.push_success(&dummy_block);

    // This log is for MESSAGE_TO_L2_CANCELLATION_STARTED_EVENT_IDENTIFIER (must check that this is
    // the second log in filters)
    let expected_message_to_l2_cancellation_started_log = encode_message_into_log(
        filters[1],
        block_number,
        &[U256::from(1), U256::from(2)],
        U256::from(0),
        None,
    );
    block_number += 1;
    asserter.push_success(&vec![expected_message_to_l2_cancellation_started_log.clone()]);
    expected_logs.push(expected_message_to_l2_cancellation_started_log);
    asserter.push_success(&dummy_block);

    // This log is for MESSAGE_TO_L2_CANCELED_EVENT_IDENTIFIER (must check that this is the third
    // log in filters)
    let expected_message_to_l2_canceled_log = encode_message_into_log(
        filters[2],
        block_number,
        &[U256::from(1), U256::from(2)],
        U256::from(0),
        None,
    );
    block_number += 1;
    asserter.push_success(&vec![expected_message_to_l2_canceled_log.clone()]);
    expected_logs.push(expected_message_to_l2_canceled_log);
    asserter.push_success(&dummy_block);

    // This log is for CONSUMED_MESSAGE_TO_L2_EVENT_IDENTIFIER (must check that this is the fourth
    // log in filters)
    let expected_consumed_message_to_l2_log = encode_message_into_log(
        filters[3],
        block_number,
        &[U256::from(1), U256::from(2)],
        U256::from(0),
        Some(U256::from(1)),
    );
    block_number += 1;
    asserter.push_success(&vec![expected_consumed_message_to_l2_log.clone()]);
    expected_logs.push(expected_consumed_message_to_l2_log);
    asserter.push_success(&dummy_block);

    // If new log types are needed, they must be added here.

    // Check that each event type has a corresponding log.
    for filter in filters {
        // Only filter for one event at a time, to make sure we trigger on all events.
        let events = base_layer.events(0..=block_number, &[filter]).await.unwrap_or_else(|_| {
            panic!("should succeed in getting events for filter: {:?}", filter)
        });
        assert!(events.len() == 1, "Expected 1 event for filter: {:?}", filter);
    }
}
