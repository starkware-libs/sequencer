use std::collections::HashSet;
use std::iter::Sum;
use std::ops::Add;

use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use serde::Serialize;
use starknet_api::core::{ClassHash, EthAddress};
use starknet_api::state::StorageKey;
use starknet_api::transaction::{EventContent, L2ToL1Payload};
use starknet_types_core::felt::Felt;

use crate::execution::contract_class::TrackedResource;
use crate::execution::entry_point::CallEntryPoint;
use crate::state::cached_state::StorageEntry;
use crate::utils::u128_from_usize;

#[cfg_attr(feature = "transaction_serde", derive(serde::Deserialize))]
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct Retdata(pub Vec<Felt>);

#[macro_export]
macro_rules! retdata {
    ( $( $x:expr ),* ) => {
        Retdata(vec![$($x),*])
    };
}

#[cfg_attr(test, derive(Clone))]
#[cfg_attr(feature = "transaction_serde", derive(serde::Deserialize))]
#[derive(Debug, Default, Eq, PartialEq, Serialize)]
pub struct OrderedEvent {
    pub order: usize,
    pub event: EventContent,
}

#[cfg_attr(test, derive(Clone))]
#[cfg_attr(feature = "transaction_serde", derive(serde::Deserialize))]
#[derive(Debug, Default, Eq, PartialEq, Serialize)]
pub struct MessageToL1 {
    pub to_address: EthAddress,
    pub payload: L2ToL1Payload,
}

#[cfg_attr(test, derive(Clone))]
#[cfg_attr(feature = "transaction_serde", derive(serde::Deserialize))]
#[derive(Debug, Default, Eq, PartialEq, Serialize)]
pub struct OrderedL2ToL1Message {
    pub order: usize,
    pub message: MessageToL1,
}

/// Represents the effects of executing a single entry point.
#[cfg_attr(test, derive(Clone))]
#[cfg_attr(feature = "transaction_serde", derive(serde::Deserialize))]
#[derive(Debug, Default, Eq, PartialEq, Serialize)]
pub struct CallExecution {
    pub retdata: Retdata,
    pub events: Vec<OrderedEvent>,
    pub l2_to_l1_messages: Vec<OrderedL2ToL1Message>,
    pub failed: bool,
    pub gas_consumed: u64,
}

#[cfg_attr(feature = "transaction_serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, Default, derive_more::AddAssign, PartialEq)]
pub struct EventSummary {
    pub n_events: usize,
    pub total_event_keys: u128,
    pub total_event_data_size: u128,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExecutionSummary {
    pub executed_class_hashes: HashSet<ClassHash>,
    pub visited_storage_entries: HashSet<StorageEntry>,
    pub l2_to_l1_payload_lengths: Vec<usize>,
    pub event_summary: EventSummary,
}

impl Add for ExecutionSummary {
    type Output = Self;

    fn add(mut self, other: Self) -> Self {
        self.executed_class_hashes.extend(other.executed_class_hashes);
        self.visited_storage_entries.extend(other.visited_storage_entries);
        self.l2_to_l1_payload_lengths.extend(other.l2_to_l1_payload_lengths);
        self.event_summary += other.event_summary;
        self
    }
}

impl Sum for ExecutionSummary {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(ExecutionSummary::default(), |acc, x| acc + x)
    }
}

/// Represents the full effects of executing an entry point, including the inner calls it invoked.
#[cfg_attr(feature = "transaction_serde", derive(serde::Deserialize))]
#[derive(Debug, Default, Eq, PartialEq, Serialize)]
pub struct CallInfo {
    pub call: CallEntryPoint,
    pub execution: CallExecution,
    pub resources: ExecutionResources,
    pub inner_calls: Vec<CallInfo>,
    pub tracked_resource: TrackedResource,

    // Additional information gathered during execution.
    pub storage_read_values: Vec<Felt>,
    pub accessed_storage_keys: HashSet<StorageKey>,
}

impl CallInfo {
    pub fn iter(&self) -> CallInfoIter<'_> {
        let call_infos = vec![self];
        CallInfoIter { call_infos }
    }

    pub fn summarize(&self) -> ExecutionSummary {
        let mut executed_class_hashes: HashSet<ClassHash> = HashSet::new();
        let mut visited_storage_entries: HashSet<StorageEntry> = HashSet::new();
        let mut event_summary = EventSummary::default();
        let mut l2_to_l1_payload_lengths = Vec::new();

        for call_info in self.iter() {
            // Class hashes.
            let class_hash =
                call_info.call.class_hash.expect("Class hash must be set after execution.");
            executed_class_hashes.insert(class_hash);

            // Storage entries.
            let call_storage_entries = call_info
                .accessed_storage_keys
                .iter()
                .map(|storage_key| (call_info.call.storage_address, *storage_key));
            visited_storage_entries.extend(call_storage_entries);

            // Messages.
            l2_to_l1_payload_lengths.extend(
                call_info
                    .execution
                    .l2_to_l1_messages
                    .iter()
                    .map(|message| message.message.payload.0.len()),
            );

            // Events.
            event_summary.n_events += call_info.execution.events.len();
            for OrderedEvent { event, .. } in call_info.execution.events.iter() {
                // TODO(barak: 18/03/2024): Once we start charging per byte
                // change to num_bytes_keys
                // and num_bytes_data.
                event_summary.total_event_data_size += u128_from_usize(event.data.0.len());
                event_summary.total_event_keys += u128_from_usize(event.keys.len());
            }
        }

        ExecutionSummary {
            executed_class_hashes,
            visited_storage_entries,
            l2_to_l1_payload_lengths,
            event_summary,
        }
    }

    pub fn summarize_many<'a>(call_infos: impl Iterator<Item = &'a CallInfo>) -> ExecutionSummary {
        call_infos.map(|call_info| call_info.summarize()).sum()
    }
}

pub struct CallInfoIter<'a> {
    call_infos: Vec<&'a CallInfo>,
}

impl<'a> Iterator for CallInfoIter<'a> {
    type Item = &'a CallInfo;

    fn next(&mut self) -> Option<Self::Item> {
        let call_info = self.call_infos.pop()?;

        // Push order is right to left.
        self.call_infos.extend(call_info.inner_calls.iter().rev());
        Some(call_info)
    }
}
