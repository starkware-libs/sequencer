use blockifier::blockifier::block::BlockInfo;
use blockifier::blockifier::stateful_validator::StatefulValidator as BlockifierStatefulValidator;
use blockifier::bouncer::BouncerConfig;
use blockifier::context::BlockContext;
use blockifier::context::ChainInfo;
use blockifier::execution::contract_class::ClassInfo;
use blockifier::state::cached_state::CachedState;
use blockifier::state::state_api::StateReader;

use blockifier::versioned_constants::VersionedConstants;
use starknet_api::core::Nonce;
use starknet_api::external_transaction::ExternalTransaction;
use starknet_api::transaction::TransactionHash;

use crate::errors::StatefulTransactionValidatorError;
use crate::errors::StatefulTransactionValidatorResult;
use crate::utils::external_tx_to_account_tx;

pub struct StatefulTransactionValidator {
    pub config: StatefulTransactionValidatorConfig,
}

impl StatefulTransactionValidator {
    pub fn run_validate(
        &self,
        // TODO(yael 17/4/24): the state_reader should be created inside the function taking
        // latest_block_number.
        state_reader: impl StateReader,
        //TODO(yael 17/4/24): the latest_block_info should be read from the storage.
        latest_block_info: BlockInfo,
        external_tx: &ExternalTransaction,
        deploy_account_tx_hash: Option<TransactionHash>,
        optional_class_info: Option<ClassInfo>,
    ) -> StatefulTransactionValidatorResult<()> {
        let state = CachedState::new(state_reader);
        let versioned_constants = VersionedConstants::latest_constants_with_overrides(
            self.config.validate_max_n_steps,
            self.config.max_recursion_depth,
        );
        let mut block_info = latest_block_info;
        block_info.block_number = block_info.block_number.next().ok_or(
            StatefulTransactionValidatorError::OutOfRangeBlockNumber {
                block_number: block_info.block_number,
            },
        )?;
        // TODO(yael 21/4/24): create the block context using pre_process_block once we will be
        // able to read the block_hash of 10 blocks ago from papyrus.
        let block_context =
            BlockContext::new_unchecked(&block_info, &self.config.chain_info, &versioned_constants);

        let mut validator = BlockifierStatefulValidator::create(
            state,
            block_context,
            self.config.max_nonce_for_validation_skip,
            BouncerConfig::max(),
        );
        let account_tx = external_tx_to_account_tx(
            external_tx,
            optional_class_info,
            &self.config.chain_info.chain_id,
        )?;
        validator.perform_validations(account_tx, deploy_account_tx_hash)?;
        Ok(())
    }
}

pub struct StatefulTransactionValidatorConfig {
    pub max_nonce_for_validation_skip: Nonce,
    pub validate_max_n_steps: u32,
    pub max_recursion_depth: usize,
    pub chain_info: ChainInfo,
}
