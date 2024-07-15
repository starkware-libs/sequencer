use indexmap::indexmap;
use primitive_types::H160;
use starknet_types_core::felt::Felt;

use super::block_hash_calculator::TransactionOutputForHash;
use crate::core::{ClassHash, CompiledClassHash, ContractAddress, EthAddress, Nonce};
use crate::state::ThinStateDiff;
use crate::transaction::{
    Fee, GasVector, L2ToL1Payload, MessageToL1, RevertedTransactionExecutionStatus,
    TransactionExecutionStatus,
};

pub(crate) fn get_transaction_output() -> TransactionOutputForHash {
    let execution_status =
        TransactionExecutionStatus::Reverted(RevertedTransactionExecutionStatus {
            revert_reason: "aborted".to_string(),
        });
    TransactionOutputForHash {
        actual_fee: Fee(99804),
        messages_sent: vec![generate_message_to_l1(34), generate_message_to_l1(56)],
        events: vec![],
        execution_status,
        gas_consumed: GasVector { l1_gas: 16580, l1_data_gas: 32 },
    }
}

pub(crate) fn generate_message_to_l1(seed: u64) -> MessageToL1 {
    MessageToL1 {
        from_address: ContractAddress::from(seed),
        to_address: EthAddress(H160::from_low_u64_be(seed + 1)),
        payload: L2ToL1Payload(vec![Felt::from(seed + 2), Felt::from(seed + 3)]),
    }
}

pub(crate) fn get_state_diff() -> ThinStateDiff {
    ThinStateDiff {
        deployed_contracts: indexmap! {
            0u64.into() => ClassHash(1u64.into()),
            2u64.into() => ClassHash(3u64.into()),
        },
        storage_diffs: indexmap! {
            4u64.into() => indexmap! {
                5u64.into() => 6u64.into(),
                7u64.into() => 8u64.into(),
            },
            9u64.into() => indexmap! {
                10u64.into() => 11u64.into(),
            },
        },
        declared_classes: indexmap! {
            ClassHash(12u64.into()) => CompiledClassHash(13u64.into()),
            ClassHash(14u64.into()) => CompiledClassHash(15u64.into()),
        },
        deprecated_declared_classes: vec![ClassHash(16u64.into())],
        nonces: indexmap! {
            17u64.into() => Nonce(18u64.into()),
        },
        replaced_classes: indexmap! {
            19u64.into() => ClassHash(20u64.into()),
        },
    }
}
