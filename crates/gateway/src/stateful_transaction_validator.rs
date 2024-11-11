use blockifier::blockifier::block::BlockInfo;
use blockifier::blockifier::stateful_validator::{
    StatefulValidator,
    StatefulValidatorResult as BlockifierStatefulValidatorResult,
};
use blockifier::bouncer::BouncerConfig;
use blockifier::context::{BlockContext, ChainInfo};
use blockifier::state::cached_state::CachedState;
use blockifier::transaction::account_transaction::AccountTransaction;
use blockifier::versioned_constants::VersionedConstants;
#[cfg(test)]
use mockall::automock;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::executable_transaction::{
    AccountTransaction as ExecutableTransaction,
    InvokeTransaction as ExecutableInvokeTransaction,
};
use starknet_gateway_types::errors::GatewaySpecError;
use starknet_types_core::felt::Felt;
use tracing::error;

use crate::config::StatefulTransactionValidatorConfig;
use crate::errors::StatefulTransactionValidatorResult;
use crate::state_reader::{MempoolStateReader, StateReaderFactory};

#[cfg(test)]
#[path = "stateful_transaction_validator_test.rs"]
mod stateful_transaction_validator_test;

pub struct StatefulTransactionValidator {
    pub config: StatefulTransactionValidatorConfig,
}

type BlockifierStatefulValidator = StatefulValidator<Box<dyn MempoolStateReader>>;

// TODO(yair): move the trait to Blockifier.
#[cfg_attr(test, automock)]
pub trait StatefulTransactionValidatorTrait {
    fn validate(
        &mut self,
        account_tx: AccountTransaction,
        skip_validate: bool,
    ) -> BlockifierStatefulValidatorResult<()>;

    fn get_nonce(
        &mut self,
        account_address: ContractAddress,
    ) -> BlockifierStatefulValidatorResult<Nonce>;
}

impl StatefulTransactionValidatorTrait for BlockifierStatefulValidator {
    fn validate(
        &mut self,
        account_tx: AccountTransaction,
        skip_validate: bool,
    ) -> BlockifierStatefulValidatorResult<()> {
        self.perform_validations(account_tx, skip_validate)
    }

    fn get_nonce(
        &mut self,
        account_address: ContractAddress,
    ) -> BlockifierStatefulValidatorResult<Nonce> {
        self.get_nonce(account_address)
    }
}

impl StatefulTransactionValidator {
    // TODO(Arni): consider separating validation from transaction conversion, as transaction
    // conversion is also relevant for the Mempool.
    pub fn run_validate<V: StatefulTransactionValidatorTrait>(
        &self,
        executable_tx: &ExecutableTransaction,
        account_nonce: Nonce,
        mut validator: V,
    ) -> StatefulTransactionValidatorResult<()> {
        let skip_validate = skip_stateful_validations(executable_tx, account_nonce);
        let account_tx = AccountTransaction::try_from(executable_tx).map_err(|error| {
            error!("Failed to convert executable transaction into account transaction: {}", error);
            GatewaySpecError::UnexpectedError { data: "Internal server error".to_owned() }
        })?;
        validator
            .validate(account_tx, skip_validate)
            .map_err(|err| GatewaySpecError::ValidationFailure { data: err.to_string() })?;
        Ok(())
    }

    pub fn instantiate_validator(
        &self,
        state_reader_factory: &dyn StateReaderFactory,
        chain_info: &ChainInfo,
    ) -> StatefulTransactionValidatorResult<BlockifierStatefulValidator> {
        // TODO(yael 6/5/2024): consider storing the block_info as part of the
        // StatefulTransactionValidator and update it only once a new block is created.
        let latest_block_info = get_latest_block_info(state_reader_factory)?;
        let state_reader = state_reader_factory.get_state_reader(latest_block_info.block_number);
        let state = CachedState::new(state_reader);
        let versioned_constants =
            VersionedConstants::get_versioned_constants(self.config.versioned_constants_overrides);
        let mut block_info = latest_block_info;
        block_info.block_number = block_info.block_number.unchecked_next();
        // TODO(yael 21/4/24): create the block context using pre_process_block once we will be
        // able to read the block_hash of 10 blocks ago from papyrus.
        let block_context = BlockContext::new(
            block_info,
            chain_info.clone(),
            versioned_constants,
            BouncerConfig::max(),
        );

        Ok(BlockifierStatefulValidator::create(state, block_context))
    }
}

// Check if validation of an invoke transaction should be skipped due to deploy_account not being
// proccessed yet. This feature is used to improve UX for users sending deploy_account + invoke at
// once.
fn skip_stateful_validations(tx: &ExecutableTransaction, account_nonce: Nonce) -> bool {
    match tx {
        ExecutableTransaction::Invoke(ExecutableInvokeTransaction { tx, .. }) => {
            // check if the transaction nonce is 1, meaning it is post deploy_account, and the
            // account nonce is zero, meaning the account was not deployed yet. The mempool also
            // verifies that the deploy_account transaction exists.
            tx.nonce() == Nonce(Felt::ONE) && account_nonce == Nonce(Felt::ZERO)
        }
        ExecutableTransaction::DeployAccount(_) | ExecutableTransaction::Declare(_) => false,
    }
}

pub fn get_latest_block_info(
    state_reader_factory: &dyn StateReaderFactory,
) -> StatefulTransactionValidatorResult<BlockInfo> {
    let state_reader = state_reader_factory.get_state_reader_from_latest_block();
    state_reader.get_block_info().map_err(|e| {
        error!("Failed to get latest block info: {}", e);
        GatewaySpecError::UnexpectedError { data: "Internal server error.".to_owned() }
    })
}
