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
use num_rational::Ratio;
use starknet_api::block::{BlockInfo, NonzeroGasPrice};
use starknet_api::core::Nonce;
use starknet_api::executable_transaction::{
    AccountTransaction as ExecutableTransaction,
    InvokeTransaction as ExecutableInvokeTransaction,
};
use starknet_api::transaction::fields::ValidResourceBounds;
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
    #[allow(clippy::result_large_err)]
    fn validate(&mut self, account_tx: AccountTransaction)
    -> BlockifierStatefulValidatorResult<()>;

    fn block_info(&self) -> &BlockInfo;
}

impl StatefulTransactionValidatorTrait for BlockifierStatefulValidator {
    #[sequencer_latency_histogram(GATEWAY_VALIDATE_TX_LATENCY, true)]
    #[allow(clippy::result_large_err)]
    fn validate(
        &mut self,
        account_tx: AccountTransaction,
    ) -> BlockifierStatefulValidatorResult<()> {
        self.perform_validations(account_tx)
    }

    fn block_info(&self) -> &BlockInfo {
        BlockifierStatefulValidator::block_info(self)
    }
}

impl StatefulTransactionValidator {
    pub fn run_transaction_validations<V: StatefulTransactionValidatorTrait>(
        &self,
        executable_tx: &ExecutableTransaction,
        account_nonce: Nonce,
        mempool_client: SharedMempoolClient,
        validator: V,
        runtime: tokio::runtime::Handle,
    ) -> StatefulTransactionValidatorResult<()> {
        self.validate_state_preconditions(executable_tx, account_nonce, &validator)?;
        self.run_validate_entry_point(
            executable_tx,
            account_nonce,
            mempool_client,
            validator,
            runtime,
        )
    }

    fn validate_state_preconditions<V: StatefulTransactionValidatorTrait>(
        &self,
        executable_tx: &ExecutableTransaction,
        account_nonce: Nonce,
        validator: &V,
    ) -> StatefulTransactionValidatorResult<()> {
        self.validate_resource_bounds(executable_tx, validator)?;
        self.validate_nonce(executable_tx, account_nonce)?;

        Ok(())
    }

    fn validate_resource_bounds<V: StatefulTransactionValidatorTrait>(
        &self,
        executable_tx: &ExecutableTransaction,
        validator: &V,
    ) -> StatefulTransactionValidatorResult<()> {
        // Skip this validation during the systems bootstrap phase.
        if self.config.validate_resource_bounds {
            // TODO(Arni): getnext_l2_gas_price from the block header.
            let previous_block_l2_gas_price =
                validator.block_info().gas_prices.strk_gas_prices.l2_gas_price;
            self.validate_tx_l2_gas_price_within_threshold(
                executable_tx.resource_bounds(),
                previous_block_l2_gas_price,
            )?;
        }

        Ok(())
    }

    fn validate_nonce(
        &self,
        executable_tx: &ExecutableTransaction,
        account_nonce: Nonce,
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

        Ok(())
    }

    fn run_validate_entry_point<V: StatefulTransactionValidatorTrait>(
        &self,
        executable_tx: &ExecutableTransaction,
        account_nonce: Nonce,
        mempool_client: SharedMempoolClient,
        mut validator: V,
        runtime: tokio::runtime::Handle,
    ) -> StatefulTransactionValidatorResult<()> {
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

    // TODO(Arni): Consider running this validation for all gas prices.
    fn validate_tx_l2_gas_price_within_threshold(
        &self,
        tx_resource_bounds: ValidResourceBounds,
        previous_block_l2_gas_price: NonzeroGasPrice,
    ) -> StatefulTransactionValidatorResult<()> {
        match tx_resource_bounds {
            ValidResourceBounds::AllResources(tx_resource_bounds) => {
                let tx_l2_gas_price = tx_resource_bounds.l2_gas.max_price_per_unit;
                let gas_price_threshold_multiplier =
                    Ratio::new(self.config.min_gas_price_percentage.into(), 100_u128);
                let threshold = (gas_price_threshold_multiplier
                    * previous_block_l2_gas_price.get().0)
                    .to_integer();
                if tx_l2_gas_price.0 < threshold {
                    return Err(StarknetError {
                        // We didn't have this kind of an error.
                        code: StarknetErrorCode::UnknownErrorCode(
                            "StarknetErrorCode.GAS_PRICE_TOO_LOW".to_string(),
                        ),
                        message: format!(
                            "Transaction L2 gas price {} is below the required threshold {}.",
                            tx_l2_gas_price, threshold
                        ),
                    });
                }
            }
            ValidResourceBounds::L1Gas(_) => {
                // No validation required for legacy transactions.
            }
        }

        Ok(())
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
