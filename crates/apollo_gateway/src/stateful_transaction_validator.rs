use apollo_gateway_types::deprecated_gateway_error::{
    KnownStarknetErrorCode,
    StarknetError,
    StarknetErrorCode,
};
use apollo_gateway_types::errors::GatewaySpecError;
use apollo_mempool_types::communication::SharedMempoolClient;
use apollo_proc_macros::sequencer_latency_histogram;
use blockifier::blockifier::stateful_validator::{
    StatefulValidator,
    StatefulValidatorResult as BlockifierStatefulValidatorResult,
};
use blockifier::blockifier_versioned_constants::VersionedConstants;
use blockifier::bouncer::BouncerConfig;
use blockifier::context::{BlockContext, ChainInfo};
use blockifier::state::cached_state::CachedState;
use blockifier::transaction::account_transaction::{AccountTransaction, ExecutionFlags};
use blockifier::transaction::transactions::enforce_fee;
#[cfg(test)]
use mockall::automock;
use starknet_api::block::BlockInfo;
use starknet_api::core::Nonce;
use starknet_api::executable_transaction::{
    AccountTransaction as ExecutableTransaction,
    InvokeTransaction as ExecutableInvokeTransaction,
};
use starknet_types_core::felt::Felt;
use tracing::{debug, error};

use crate::config::StatefulTransactionValidatorConfig;
use crate::errors::{mempool_client_err_to_deprecated_gw_err, StatefulTransactionValidatorResult};
use crate::metrics::GATEWAY_VALIDATE_TX_LATENCY;
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
    fn validate(&mut self, account_tx: AccountTransaction)
    -> BlockifierStatefulValidatorResult<()>;
}

impl StatefulTransactionValidatorTrait for BlockifierStatefulValidator {
    #[sequencer_latency_histogram(GATEWAY_VALIDATE_TX_LATENCY, true)]
    fn validate(
        &mut self,
        account_tx: AccountTransaction,
    ) -> BlockifierStatefulValidatorResult<()> {
        self.perform_validations(account_tx)
    }
}

impl StatefulTransactionValidator {
    pub fn run_validate<V: StatefulTransactionValidatorTrait>(
        &self,
        executable_tx: &ExecutableTransaction,
        account_nonce: Nonce,
        mempool_client: SharedMempoolClient,
        mut validator: V,
        runtime: tokio::runtime::Handle,
    ) -> StatefulTransactionValidatorResult<()> {
        if !self.is_valid_nonce(executable_tx, account_nonce) {
            let tx_nonce = executable_tx.nonce();
            debug!(
                "Transaction nonce is invalid. Transaction nonce: {tx_nonce}, account_nonce: \
                 {account_nonce}",
            );
            return Err(StarknetError {
                code: StarknetErrorCode::KnownErrorCode(
                    KnownStarknetErrorCode::InvalidTransactionNonce,
                ),
                message: format!(
                    "Invalid transaction nonce. Expected: {account_nonce}, got: {tx_nonce}."
                ),
            });
        }

        let skip_validate =
            skip_stateful_validations(executable_tx, account_nonce, mempool_client, runtime)?;
        let only_query = false;
        let charge_fee = enforce_fee(executable_tx, only_query);
        let strict_nonce_check = false;
        let execution_flags =
            ExecutionFlags { only_query, charge_fee, validate: !skip_validate, strict_nonce_check };

        let account_tx = AccountTransaction { tx: executable_tx.clone(), execution_flags };
        validator.validate(account_tx).map_err(|e| StarknetError {
            code: StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::ValidateFailure),
            message: e.to_string(),
        })?;
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
        let versioned_constants = VersionedConstants::get_versioned_constants(
            self.config.versioned_constants_overrides.clone(),
        );
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

    fn is_valid_nonce(&self, executable_tx: &ExecutableTransaction, account_nonce: Nonce) -> bool {
        let incoming_tx_nonce = executable_tx.nonce();

        // Declare transactions must have the same nonce as the account nonce.
        if self.config.reject_future_declare_txs
            && matches!(executable_tx, ExecutableTransaction::Declare(_))
        {
            return incoming_tx_nonce == account_nonce;
        }

        let max_allowed_nonce =
            Nonce(account_nonce.0 + Felt::from(self.config.max_allowed_nonce_gap));
        account_nonce <= incoming_tx_nonce && incoming_tx_nonce <= max_allowed_nonce
    }
}

/// Check if validation of an invoke transaction should be skipped due to deploy_account not being
/// processed yet. This feature is used to improve UX for users sending deploy_account + invoke at
/// once.
fn skip_stateful_validations(
    tx: &ExecutableTransaction,
    account_nonce: Nonce,
    mempool_client: SharedMempoolClient,
    runtime: tokio::runtime::Handle,
) -> StatefulTransactionValidatorResult<bool> {
    if let ExecutableTransaction::Invoke(ExecutableInvokeTransaction { tx, .. }) = tx {
        // check if the transaction nonce is 1, meaning it is post deploy_account, and the
        // account nonce is zero, meaning the account was not deployed yet.
        if tx.nonce() == Nonce(Felt::ONE) && account_nonce == Nonce(Felt::ZERO) {
            // We verify that a deploy_account transaction exists for this account. It is sufficient
            // to check if the account exists in the mempool since it means that either it has a
            // deploy_account transaction or transactions with future nonces that passed
            // validations.
            return runtime
                .block_on(mempool_client.account_tx_in_pool_or_recent_block(tx.sender_address()))
                .map_err(mempool_client_err_to_deprecated_gw_err);
        }
    }

    Ok(false)
}

pub fn get_latest_block_info(
    state_reader_factory: &dyn StateReaderFactory,
) -> StatefulTransactionValidatorResult<BlockInfo> {
    let state_reader = state_reader_factory
        .get_state_reader_from_latest_block()
        .map_err(|e| {
            error!("Failed to get state reader from latest block: {}", e);
            GatewaySpecError::UnexpectedError { data: "Internal server error.".to_owned() }
        })
        .map_err(|e| StarknetError::internal(&e.to_string()))?;
    state_reader.get_block_info().map_err(|e| {
        error!("Failed to get latest block info: {}", e);
        StarknetError::internal(&e.to_string())
    })
}
