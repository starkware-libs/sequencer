use std::collections::BTreeMap;
use std::sync::Arc;

use apollo_config::dumping::{prepend_sub_config_name, ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockInfo, BlockNumber, BlockTimestamp, FeeType, GasPriceVector};
use starknet_api::core::{ChainId, ContractAddress};
use starknet_api::execution_resources::GasAmount;
use starknet_api::transaction::fields::{
    AllResourceBounds,
    Fee,
    GasVectorComputationMode,
    Tip,
    ValidResourceBounds,
};

use crate::blockifier_versioned_constants::VersionedConstants;
use crate::bouncer::BouncerConfig;
use crate::execution::call_info::CallInfo;
use crate::execution::common_hints::ExecutionMode;
use crate::transaction::objects::{
    CurrentTransactionInfo,
    HasRelatedFeeType,
    TransactionInfo,
    TransactionInfoCreator,
};

#[derive(Clone, Debug)]
pub struct TransactionContext {
    pub block_context: Arc<BlockContext>,
    pub tx_info: TransactionInfo,
}

impl TransactionContext {
    pub fn fee_token_address(&self) -> ContractAddress {
        self.block_context.chain_info.fee_token_address(&self.tx_info.fee_type())
    }
    pub fn is_sequencer_the_sender(&self) -> bool {
        self.tx_info.sender_address() == self.block_context.block_info.sequencer_address
    }
    pub fn get_gas_vector_computation_mode(&self) -> GasVectorComputationMode {
        self.tx_info.gas_mode()
    }
    pub fn get_gas_prices(&self) -> &GasPriceVector {
        self.block_context.block_info.gas_prices.gas_price_vector(&self.tx_info.fee_type())
    }
    pub fn sierra_gas_limit(&self, mode: &ExecutionMode) -> GasAmount {
        self.block_context.versioned_constants.sierra_gas_limit(mode)
    }

    /// Returns the initial Sierra gas of the transaction.
    /// This value is used to limit the transaction's run.
    pub fn initial_sierra_gas(&self) -> GasAmount {
        match &self.tx_info {
            TransactionInfo::Deprecated(_)
            | TransactionInfo::Current(CurrentTransactionInfo {
                resource_bounds: ValidResourceBounds::L1Gas(_),
                ..
            }) => self.block_context.versioned_constants.initial_gas_no_user_l2_bound(),
            TransactionInfo::Current(CurrentTransactionInfo {
                resource_bounds: ValidResourceBounds::AllResources(AllResourceBounds { l2_gas, .. }),
                ..
            }) => l2_gas.max_amount,
        }
    }

    pub fn effective_tip(&self) -> Tip {
        if self.block_context.versioned_constants.enable_tip {
            match &self.tx_info {
                TransactionInfo::Current(current_tx_info) => current_tx_info.tip,
                TransactionInfo::Deprecated(_) => Tip::ZERO,
            }
        } else {
            Tip::ZERO
        }
    }

    pub fn max_possible_fee(&self) -> Fee {
        match &self.tx_info {
            TransactionInfo::Current(current_tx_info) => {
                current_tx_info.resource_bounds.max_possible_fee(self.effective_tip())
            }
            TransactionInfo::Deprecated(deprecated_tx_info) => deprecated_tx_info.max_fee,
        }
    }
}

pub struct GasCounter {
    pub(crate) spent_gas: GasAmount,
    pub(crate) remaining_gas: GasAmount,
}

impl GasCounter {
    pub(crate) fn new(initial_gas: GasAmount) -> Self {
        GasCounter { spent_gas: GasAmount(0), remaining_gas: initial_gas }
    }

    fn spend(&mut self, amount: GasAmount) {
        self.spent_gas = self.spent_gas.checked_add(amount).expect("Gas overflow");
        self.remaining_gas = self
            .remaining_gas
            .checked_sub(amount)
            .expect("Overuse of gas; should have been caught earlier");
    }

    /// Limits the amount of gas that can be used (in validate\execute) by the given global limit.
    pub(crate) fn limit_usage(&self, amount: GasAmount) -> u64 {
        self.remaining_gas.min(amount).0
    }

    pub(crate) fn subtract_used_gas(&mut self, call_info: &CallInfo) {
        self.spend(GasAmount(call_info.execution.gas_consumed));
    }
}

#[derive(Clone, Debug)]
pub struct BlockContext {
    // TODO(Yoni, 1/10/2024): consider making these fields public.
    pub block_info: BlockInfo,
    pub chain_info: ChainInfo,
    pub versioned_constants: VersionedConstants,
    pub bouncer_config: BouncerConfig,
}

impl BlockContext {
    pub fn new(
        block_info: BlockInfo,
        chain_info: ChainInfo,
        versioned_constants: VersionedConstants,
        bouncer_config: BouncerConfig,
    ) -> Self {
        BlockContext { block_info, chain_info, versioned_constants, bouncer_config }
    }

    pub fn block_info(&self) -> &BlockInfo {
        &self.block_info
    }

    pub fn chain_info(&self) -> &ChainInfo {
        &self.chain_info
    }

    pub fn versioned_constants(&self) -> &VersionedConstants {
        &self.versioned_constants
    }

    pub fn to_tx_context(
        &self,
        tx_info_creator: &impl TransactionInfoCreator,
    ) -> TransactionContext {
        TransactionContext {
            block_context: Arc::new(self.clone()),
            tx_info: tx_info_creator.create_tx_info(),
        }
    }

    pub fn block_info_for_validate(&self) -> BlockInfo {
        let block_number = self.block_info.block_number.0;
        let block_timestamp = self.block_info.block_timestamp.0;
        // Round down to the nearest multiple of validate_block_number_rounding.
        let validate_block_number_rounding =
            self.versioned_constants.get_validate_block_number_rounding();
        let rounded_block_number =
            (block_number / validate_block_number_rounding) * validate_block_number_rounding;
        // Round down to the nearest multiple of validate_timestamp_rounding.
        let validate_timestamp_rounding =
            self.versioned_constants.get_validate_timestamp_rounding();
        let rounded_timestamp =
            (block_timestamp / validate_timestamp_rounding) * validate_timestamp_rounding;
        BlockInfo {
            block_number: BlockNumber(rounded_block_number),
            block_timestamp: BlockTimestamp(rounded_timestamp),
            sequencer_address: 0_u128.into(),
            // TODO(Yoni): consider setting here trivial prices if and when this field is exposed.
            gas_prices: self.block_info.gas_prices.clone(),
            use_kzg_da: self.block_info.use_kzg_da,
        }
    }

    /// Test util to allow overriding block gas limits.
    #[cfg(any(test, feature = "testing"))]
    pub fn set_sierra_gas_limits(
        &mut self,
        execute_max_gas: Option<GasAmount>,
        validate_max_gas: Option<GasAmount>,
    ) {
        let mut new_os_constants = (*self.versioned_constants.os_constants).clone();
        if let Some(execute_max_gas) = execute_max_gas {
            new_os_constants.execute_max_sierra_gas = execute_max_gas;
        }
        if let Some(validate_max_gas) = validate_max_gas {
            new_os_constants.validate_max_sierra_gas = validate_max_gas;
        }
        self.versioned_constants.os_constants = std::sync::Arc::new(new_os_constants);
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ChainInfo {
    pub chain_id: ChainId,
    pub fee_token_addresses: FeeTokenAddresses,
}

impl ChainInfo {
    // TODO(Gilad): since fee_type comes from TransactionInfo, we can move this method into
    // TransactionContext, which has both the chain_info (through BlockContext) and the tx_info.
    // That is, add to BlockContext with the signature `pub fn fee_token_address(&self)`.
    pub fn fee_token_address(&self, fee_type: &FeeType) -> ContractAddress {
        self.fee_token_addresses.get_by_fee_type(fee_type)
    }
}

impl Default for ChainInfo {
    fn default() -> Self {
        ChainInfo {
            // TODO(guyn): should we remove the default value for chain_id?
            chain_id: ChainId::Other("0x0".to_string()),
            fee_token_addresses: FeeTokenAddresses::default(),
        }
    }
}

impl SerializeConfig for ChainInfo {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let members = BTreeMap::from_iter([ser_param(
            "chain_id",
            &self.chain_id,
            "The chain ID of the StarkNet chain.",
            ParamPrivacyInput::Public,
        )]);

        vec![
            members,
            prepend_sub_config_name(self.fee_token_addresses.dump(), "fee_token_addresses"),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct FeeTokenAddresses {
    pub strk_fee_token_address: ContractAddress,
    pub eth_fee_token_address: ContractAddress,
}

impl FeeTokenAddresses {
    pub fn get_by_fee_type(&self, fee_type: &FeeType) -> ContractAddress {
        match fee_type {
            FeeType::Strk => self.strk_fee_token_address,
            FeeType::Eth => self.eth_fee_token_address,
        }
    }
}

impl SerializeConfig for FeeTokenAddresses {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "strk_fee_token_address",
                &self.strk_fee_token_address,
                "Address of the STRK fee token.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "eth_fee_token_address",
                &self.eth_fee_token_address,
                "Address of the ETH fee token.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}
