//! Local copies of types from apollo_starknet_client for use in batcher, prefixed with
//! StarknetClient.
use std::collections::HashMap;

// TODO(noamsp): find a way to share the TransactionReceipt from apollo_starknet_client and
// remove this module.
use blockifier::transaction::objects::TransactionExecutionInfo;
use serde::{Deserialize, Serialize};
use starknet_api::block::{
    BlockHash,
    BlockStatus,
    BlockTimestamp,
    GasPricePerToken,
    StarknetVersion,
};
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::core::{ContractAddress, EntryPointSelector, EthAddress};
use starknet_api::data_availability::L1DataAvailabilityMode;
use starknet_api::execution_resources::GasVector;
use starknet_api::hash::StarkHash;
use starknet_api::state::ThinStateDiff;
use starknet_api::transaction::fields::Fee;
use starknet_api::transaction::{
    Event,
    L1ToL2Payload,
    L2ToL1Payload,
    TransactionHash,
    TransactionOffsetInBlock,
};

#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct L1ToL2Nonce(pub StarkHash);

#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct L1ToL2Message {
    pub from_address: EthAddress,
    pub to_address: ContractAddress,
    pub selector: EntryPointSelector,
    pub payload: L1ToL2Payload,
    #[serde(default)]
    pub nonce: L1ToL2Nonce,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct L2ToL1Message {
    pub from_address: ContractAddress,
    pub to_address: EthAddress,
    pub payload: L2ToL1Payload,
}

// Note: the serialization is different from the one in starknet_api.
#[derive(Hash, Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub enum Builtin {
    #[serde(rename = "range_check_builtin")]
    RangeCheck,
    #[serde(rename = "pedersen_builtin")]
    Pedersen,
    #[serde(rename = "poseidon_builtin")]
    Poseidon,
    #[serde(rename = "ec_op_builtin")]
    EcOp,
    #[serde(rename = "ecdsa_builtin")]
    Ecdsa,
    #[serde(rename = "bitwise_builtin")]
    Bitwise,
    #[serde(rename = "keccak_builtin")]
    Keccak,
    // Note: in starknet_api this variant doesn't exist.
    #[serde(rename = "output_builtin")]
    Output,
    #[serde(rename = "segment_arena_builtin")]
    SegmentArena,
    #[serde(rename = "add_mod_builtin")]
    AddMod,
    #[serde(rename = "mul_mod_builtin")]
    MulMod,
    #[serde(rename = "range_check96_builtin")]
    RangeCheck96,
}

/// The execution resources used by a transaction.
#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ExecutionResources {
    // Note: in starknet_api this field is named `steps`
    pub n_steps: u64,
    pub builtin_instance_counter: HashMap<Builtin, u64>,
    // Note: in starknet_api this field is named `memory_holes`
    pub n_memory_holes: u64,
    // This field is missing in blocks created before v0.13.1, even if the feeder gateway is of
    // that version
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_availability: Option<GasVector>,
    // This field is missing in blocks created before v0.13.2, even if the feeder gateway is of
    // that version
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_gas_consumed: Option<GasVector>,
}

/// Transaction execution status.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord, Default)]
pub enum TransactionExecutionStatus {
    #[serde(rename = "SUCCEEDED")]
    #[default]
    Succeeded,
    #[serde(rename = "REVERTED")]
    Reverted,
}

// TODO(Arni): Consider deleting derive default for this type. Same for members of this struct.
#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StarknetClientTransactionReceipt {
    pub transaction_index: TransactionOffsetInBlock,
    pub transaction_hash: TransactionHash,
    #[serde(default)]
    pub l1_to_l2_consumed_message: Option<L1ToL2Message>,
    pub l2_to_l1_messages: Vec<L2ToL1Message>,
    pub events: Vec<Event>,
    #[serde(default)]
    pub execution_resources: ExecutionResources,
    pub actual_fee: Fee,
    // TODO(Yair): Check if we can remove the serde(default).
    #[serde(default)]
    pub execution_status: TransactionExecutionStatus,
    // Note that in starknet_api this field is named `revert_reason`.
    // Assumption: if the transaction execution status is Succeeded, then revert_error is None, and
    // if the transaction execution status is Reverted, then revert_error is Some.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revert_error: Option<String>,
}

// Conversion logic from blockifier types to StarknetClient types.
impl
    From<(
        TransactionHash,
        // TODO(Arni): change the type of this parameter to TransactionOffsetInBlock
        usize,
        &TransactionExecutionInfo,
    )> for StarknetClientTransactionReceipt
{
    fn from(
        (tx_hash, tx_index, _tx_execution_info): (
            TransactionHash,
            usize,
            &TransactionExecutionInfo,
        ),
    ) -> Self {
        Self {
            transaction_index: TransactionOffsetInBlock(tx_index),
            transaction_hash: tx_hash,
            // TODO(Arni): Fill this up. This is relevant only for L1 handler transactions.
            l1_to_l2_consumed_message: None,
            ..Default::default()
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, Eq, PartialEq, Hash)]
pub struct CendePreConfirmedTransaction {
    pub transaction_hash: TransactionHash,
    // TODO(noamsp): add the relevant fields.
}

impl From<InternalConsensusTransaction> for CendePreConfirmedTransaction {
    fn from(transaction: InternalConsensusTransaction) -> Self {
        Self { transaction_hash: transaction.tx_hash() }
    }
}

#[derive(Serialize, Clone)]
pub struct CendeBlockMetdata {
    pub parent_block_hash: BlockHash,
    pub status: BlockStatus,
    pub starknet_version: StarknetVersion,
    pub l1_da_mode: L1DataAvailabilityMode,
    pub l1_gas_price: GasPricePerToken,
    pub l1_data_gas_price: GasPricePerToken,
    pub l2_gas_price: GasPricePerToken,
    pub timestamp: BlockTimestamp,
    pub sequencer_address: ContractAddress,
}

// TODO(noamsp): remove this method once we have the all the required info.
impl CendeBlockMetdata {
    pub fn empty_pending() -> Self {
        Self {
            parent_block_hash: BlockHash::default(),
            status: BlockStatus::Pending,
            starknet_version: StarknetVersion::default(),
            l1_da_mode: L1DataAvailabilityMode::default(),
            l1_gas_price: GasPricePerToken::default(),
            l1_data_gas_price: GasPricePerToken::default(),
            l2_gas_price: GasPricePerToken::default(),
            timestamp: BlockTimestamp::default(),
            sequencer_address: ContractAddress::default(),
        }
    }
}

#[derive(Serialize)]
pub struct CendePreConfirmedBlock {
    #[serde(flatten)]
    pub metadata: CendeBlockMetdata,
    pub transactions: Vec<CendePreConfirmedTransaction>,
    pub transaction_receipts: Vec<Option<StarknetClientTransactionReceipt>>,
    pub transaction_state_diffs: Vec<Option<ThinStateDiff>>,
}
