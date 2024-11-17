use std::collections::HashSet;
use std::iter::Sum;
use std::ops::Add;

use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use serde::Serialize;
use starknet_api::core::{ClassHash, ContractAddress, EthAddress};
use starknet_api::execution_resources::GasAmount;
use starknet_api::state::StorageKey;
use starknet_api::transaction::{EventContent, L2ToL1Payload};
use starknet_types_core::felt::Felt;

use crate::execution::contract_class::TrackedResource;
use crate::execution::entry_point::CallEntryPoint;
use crate::state::cached_state::StorageEntry;
use crate::utils::u64_from_usize;
use crate::versioned_constants::VersionedConstants;

#[cfg_attr(feature = "transaction_serde", derive(serde::Deserialize))]
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct Retdata(pub Vec<Felt>);

#[macro_export]
macro_rules! retdata {
    ( $( $x:expr ),* ) => {
        $crate::execution::call_info::Retdata(vec![$($x),*])
    };
}

#[cfg_attr(any(test, feature = "testing"), derive(Clone))]
#[cfg_attr(feature = "transaction_serde", derive(serde::Deserialize))]
#[derive(Debug, Default, Eq, PartialEq, Serialize)]
pub struct OrderedEvent {
    pub order: usize,
    pub event: EventContent,
}

#[cfg_attr(any(test, feature = "testing"), derive(Clone))]
#[cfg_attr(feature = "transaction_serde", derive(serde::Deserialize))]
#[derive(Debug, Default, Eq, PartialEq, Serialize)]
pub struct MessageToL1 {
    pub to_address: EthAddress,
    pub payload: L2ToL1Payload,
}

#[cfg_attr(any(test, feature = "testing"), derive(Clone))]
#[cfg_attr(feature = "transaction_serde", derive(serde::Deserialize))]
#[derive(Debug, Default, Eq, PartialEq, Serialize)]
pub struct OrderedL2ToL1Message {
    pub order: usize,
    pub message: MessageToL1,
}

/// Represents the effects of executing a single entry point.
#[cfg_attr(any(test, feature = "testing"), derive(Clone))]
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
    pub total_event_keys: u64,
    pub total_event_data_size: u64,
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

/// L2 resources counted for fee charge.
/// When all execution will be using gas (no VM mode), this should be removed, and the gas_consumed
/// field should be used for fee collection.
#[cfg_attr(feature = "transaction_serde", derive(serde::Deserialize))]
#[derive(Clone, Debug, Default, Serialize, Eq, PartialEq)]
pub struct ChargedResources {
    pub vm_resources: ExecutionResources, // Counted in CairoSteps mode calls.
    pub gas_for_fee: GasAmount,           // Counted in SierraGas mode calls.
}

impl ChargedResources {
    pub fn from_execution_resources(resources: ExecutionResources) -> Self {
        Self { vm_resources: resources, ..Default::default() }
    }
}

/// Returns the total gas_for_fee used in the given validate and execute calls.
pub fn gas_for_fee_from_call_infos(
    validate: &Option<CallInfo>,
    execute: &Option<CallInfo>,
) -> GasAmount {
    let validate_gas_amount = validate
        .as_ref()
        .map(|call_info| call_info.charged_resources.gas_for_fee)
        .unwrap_or(GasAmount(0));
    let execute_gas_amount = execute
        .as_ref()
        .map(|call_info| call_info.charged_resources.gas_for_fee)
        .unwrap_or(GasAmount(0));
    validate_gas_amount.checked_add(execute_gas_amount).unwrap_or_else(|| {
        panic!(
            "Gas for fee overflowed: tried to add {execute_gas_amount} to \
             {validate_gas_amount}",
        )
    })
}

/// Represents the full effects of executing an entry point, including the inner calls it invoked.
#[cfg_attr(any(test, feature = "testing"), derive(Clone))]
#[cfg_attr(feature = "transaction_serde", derive(serde::Deserialize))]
#[derive(Debug, Default, Eq, PartialEq, Serialize)]
pub struct CallInfo {
    pub call: CallEntryPoint,
    pub execution: CallExecution,
    pub inner_calls: Vec<CallInfo>,
    pub tracked_resource: TrackedResource,
    pub charged_resources: ChargedResources,

    // Additional information gathered during execution.
    pub storage_read_values: Vec<Felt>,
    pub accessed_storage_keys: HashSet<StorageKey>,
    pub read_class_hash_values: Vec<ClassHash>,
    pub accessed_contract_addresses: HashSet<ContractAddress>,
}

impl CallInfo {
    pub fn iter(&self) -> CallInfoIter<'_> {
        let call_infos = vec![self];
        CallInfoIter { call_infos }
    }

    fn specific_event_summary(&self) -> EventSummary {
        let mut event_summary =
            EventSummary { n_events: self.execution.events.len(), ..Default::default() };
        for OrderedEvent { event, .. } in self.execution.events.iter() {
            event_summary.total_event_data_size += u64_from_usize(event.data.0.len());
            event_summary.total_event_keys += u64_from_usize(event.keys.len());
        }
        event_summary
    }

    pub fn summarize(&self, versioned_constants: &VersionedConstants) -> ExecutionSummary {
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

            // Events: all event resources in the execution tree, unless executing a 0.13.1 block.
            if !versioned_constants.ignore_inner_events_resources {
                event_summary += call_info.specific_event_summary();
            }
        }

        if versioned_constants.ignore_inner_events_resources {
            // For reexecution of 0.13.1 blocks, we ignore inner events resources - only outermost
            // event data will be processed.
            event_summary = self.specific_event_summary();
        }

        ExecutionSummary {
            executed_class_hashes,
            visited_storage_entries,
            l2_to_l1_payload_lengths,
            event_summary,
        }
    }

    pub fn summarize_many<'a>(
        call_infos: impl Iterator<Item = &'a CallInfo>,
        versioned_constants: &VersionedConstants,
    ) -> ExecutionSummary {
        call_infos.map(|call_info| call_info.summarize(versioned_constants)).sum()
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
