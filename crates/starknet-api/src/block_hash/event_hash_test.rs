use super::calculate_event_hash;
use crate::core::{ContractAddress, EventCommitment, PatriciaKey};
use crate::hash::{StarkFelt, StarkHash};
use crate::transaction::{Event, EventContent, EventData, EventKey, TransactionHash};
use crate::{contract_address, patricia_key, stark_felt};

#[test]
fn test_event_hash_regression() {
    let event = Event {
        from_address: contract_address!(10_u8),
        content: EventContent {
            keys: [2_u8, 3].iter().map(|key| EventKey(stark_felt!(*key))).collect(),
            data: EventData([4_u8, 5, 6].into_iter().map(StarkFelt::from).collect()),
        },
    };
    let tx_hash = TransactionHash(stark_felt!("0x1234"));

    let expected_hash = EventCommitment(
        StarkFelt::try_from("0x367807f532742a4dcbe2d8a47b974b22dd7496faa75edc64a3a5fdb6709057")
            .unwrap(),
    );

    assert_eq!(expected_hash, calculate_event_hash(&event, &tx_hash));
}
