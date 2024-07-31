use blockifier::blockifier::block::BlockInfo;
use blockifier::blockifier::stateful_validator::{
    StatefulValidator,
    StatefulValidatorResult as BlockifierStatefulValidatorResult,
};
use blockifier::bouncer::BouncerConfig;
use blockifier::context::BlockContext;
use blockifier::execution::contract_class::ClassInfo;
use blockifier::state::cached_state::CachedState;
use blockifier::transaction::account_transaction::AccountTransaction;
use blockifier::versioned_constants::VersionedConstants;
#[cfg(test)]
use mockall::automock;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::rpc_transaction::{RpcInvokeTransaction, RpcTransaction};
use starknet_api::transaction::TransactionHash;
use starknet_types_core::felt::Felt;

use crate::config::StatefulTransactionValidatorConfig;
use crate::errors::StatefulTransactionValidatorResult;
use crate::state_reader::{MempoolStateReader, StateReaderFactory};
use crate::utils::{external_tx_to_account_tx, get_sender_address, get_tx_hash};

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
    pub fn run_validate<V: StatefulTransactionValidatorTrait>(
        &self,
        external_tx: &RpcTransaction,
        optional_class_info: Option<ClassInfo>,
        mut validator: V,
    ) -> StatefulTransactionValidatorResult<TransactionHash> {
        let account_tx = external_tx_to_account_tx(
            external_tx,
            optional_class_info,
            &self.config.chain_info.chain_id,
        )?;
        let tx_hash = get_tx_hash(&account_tx);
        let account_nonce = validator.get_nonce(get_sender_address(external_tx))?;
        let skip_validate = skip_stateful_validations(external_tx, account_nonce);
        validator.validate(account_tx, skip_validate)?;
        Ok(tx_hash)
    }

    pub fn instantiate_validator(
        &self,
        state_reader_factory: &dyn StateReaderFactory,
    ) -> StatefulTransactionValidatorResult<BlockifierStatefulValidator> {
        // TODO(yael 6/5/2024): consider storing the block_info as part of the
        // StatefulTransactionValidator and update it only once a new block is created.
        let latest_block_info = get_latest_block_info(state_reader_factory)?;
        let state_reader = state_reader_factory.get_state_reader(latest_block_info.block_number);
        let state = CachedState::new(state_reader);
        let versioned_constants = VersionedConstants::latest_constants_with_overrides(
            self.config.validate_max_n_steps,
            self.config.max_recursion_depth,
        );
        let mut block_info = latest_block_info;
        block_info.block_number = block_info.block_number.unchecked_next();
        // TODO(yael 21/4/24): create the block context using pre_process_block once we will be
        // able to read the block_hash of 10 blocks ago from papyrus.
        let block_context = BlockContext::new(
            block_info,
            self.config.chain_info.clone(),
            versioned_constants,
            BouncerConfig::max(),
        );

        Ok(BlockifierStatefulValidator::create(state, block_context))
    }
}

// Check if validation of an invoke transaction should be skipped due to deploy_account not being
// proccessed yet. This feature is used to improve UX for users sending deploy_account + invoke at
// once.
fn skip_stateful_validations(tx: &RpcTransaction, account_nonce: Nonce) -> bool {
    match tx {
        RpcTransaction::Invoke(RpcInvokeTransaction::V3(tx)) => {
            // check if the transaction nonce is 1, meaning it is post deploy_account, and the
            // account nonce is zero, meaning the account was not deployed yet. The mempool also
            // verifies that the deploy_account transaction exists.
            tx.nonce == Nonce(Felt::ONE) && account_nonce == Nonce(Felt::ZERO)
        }
        RpcTransaction::DeployAccount(_) | RpcTransaction::Declare(_) => false,
    }
}

pub fn get_latest_block_info(
    state_reader_factory: &dyn StateReaderFactory,
) -> StatefulTransactionValidatorResult<BlockInfo> {
    let state_reader = state_reader_factory.get_state_reader_from_latest_block();
    Ok(state_reader.get_block_info()?)
}
