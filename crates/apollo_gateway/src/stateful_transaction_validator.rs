use std::sync::Arc;

use apollo_gateway_config::config::StatefulTransactionValidatorConfig;
use apollo_gateway_types::deprecated_gateway_error::{
    KnownStarknetErrorCode,
    StarknetError,
    StarknetErrorCode,
};
use apollo_gateway_types::errors::GatewaySpecError;
use apollo_mempool_types::communication::SharedMempoolClient;
use apollo_mempool_types::mempool_types::ValidationArgs;
use apollo_proc_macros::sequencer_latency_histogram;
use async_trait::async_trait;
use blockifier::blockifier::stateful_validator::{StatefulValidator, StatefulValidatorTrait};
use blockifier::blockifier_versioned_constants::VersionedConstants;
use blockifier::bouncer::BouncerConfig;
use blockifier::context::{BlockContext, ChainInfo};
use blockifier::state::cached_state::CachedState;
use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier::state::state_reader_and_contract_manager::StateReaderAndContractManager;
use blockifier::transaction::account_transaction::{AccountTransaction, ExecutionFlags};
use blockifier::transaction::transactions::enforce_fee;
use num_rational::Ratio;
use starknet_api::block::NonzeroGasPrice;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::executable_transaction::{
    AccountTransaction as ExecutableTransaction,
    InvokeTransaction as ExecutableInvokeTransaction,
};
use starknet_api::transaction::fields::ValidResourceBounds;
use starknet_types_core::felt::Felt;
use tracing::{debug, Span};

use crate::errors::{mempool_client_err_to_deprecated_gw_err, StatefulTransactionValidatorResult};
use crate::gateway_fixed_block_state_reader::GatewayFixedBlockStateReader;
use crate::metrics::{GATEWAY_CLASS_CACHE_METRICS, GATEWAY_VALIDATE_TX_LATENCY};
use crate::state_reader::{GatewayStateReaderWithCompiledClasses, StateReaderFactory};

#[cfg(test)]
#[path = "stateful_transaction_validator_test.rs"]
mod stateful_transaction_validator_test;

type BlockifierStatefulValidator = StatefulValidator<
    StateReaderAndContractManager<Box<dyn GatewayStateReaderWithCompiledClasses>>,
>;

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait StatefulTransactionValidatorFactoryTrait: Send + Sync {
    async fn instantiate_validator(
        &self,
    ) -> StatefulTransactionValidatorResult<Box<dyn StatefulTransactionValidatorTrait>>;
}

pub struct StatefulTransactionValidatorFactory<GenericStateReaderFactory>
where
    GenericStateReaderFactory: StateReaderFactory,
{
    pub config: StatefulTransactionValidatorConfig,
    pub chain_info: ChainInfo,
    pub state_reader_factory: Arc<GenericStateReaderFactory>,
    pub contract_class_manager: ContractClassManager,
}

#[async_trait]
impl<GenericStateReaderFactory> StatefulTransactionValidatorFactoryTrait
    for StatefulTransactionValidatorFactory<GenericStateReaderFactory>
where
    GenericStateReaderFactory: StateReaderFactory,
{
    async fn instantiate_validator(
        &self,
    ) -> StatefulTransactionValidatorResult<Box<dyn StatefulTransactionValidatorTrait>> {
        // TODO(yael 6/5/2024): consider storing the block_info as part of the
        // StatefulTransactionValidator and update it only once a new block is created.
        let (blockifier_state_reader, gateway_fixed_block_state_reader) = self
            .state_reader_factory
            .get_blockifier_state_reader_and_gateway_fixed_block_from_latest_block()
            .await
            .map_err(|err| GatewaySpecError::UnexpectedError {
                data: format!("Internal server error: {err}"),
            })
            .map_err(|e| {
                StarknetError::internal_with_logging(
                    "Failed to get state reader from latest block",
                    e,
                )
            })?;
        // Convert concrete type to trait object. This is safe because
        // StateReaderWithCompiledClasses implements GatewayStateReaderWithCompiledClasses.
        let boxed_state_reader: Box<dyn GatewayStateReaderWithCompiledClasses> =
            Box::new(blockifier_state_reader);
        let state_reader_and_contract_manager = StateReaderAndContractManager::new(
            boxed_state_reader,
            self.contract_class_manager.clone(),
            Some(GATEWAY_CLASS_CACHE_METRICS),
        );

        Ok(Box::new(StatefulTransactionValidator::new(
            self.config.clone(),
            self.chain_info.clone(),
            state_reader_and_contract_manager,
            gateway_fixed_block_state_reader,
        )))
    }
}

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait StatefulTransactionValidatorTrait: Send {
    async fn extract_state_nonce_and_run_validations(
        &mut self,
        executable_tx: &ExecutableTransaction,
        mempool_client: SharedMempoolClient,
    ) -> StatefulTransactionValidatorResult<Nonce>;
}

pub struct StatefulTransactionValidator<FixedBlockStateReader: GatewayFixedBlockStateReader> {
    config: StatefulTransactionValidatorConfig,
    chain_info: ChainInfo,
    // Consumed when running the CPU-heavy blockifier validation.
    // TODO(Itamar): The whole `StatefulTransactionValidator` is never used after
    // `state_reader_and_contract_manager` is taken. Make it non-optional and discard the
    // instance after use.
    state_reader_and_contract_manager:
        Option<StateReaderAndContractManager<Box<dyn GatewayStateReaderWithCompiledClasses>>>,
    gateway_fixed_block_state_reader: FixedBlockStateReader,
}

#[async_trait]
impl<FixedBlockStateReader: GatewayFixedBlockStateReader> StatefulTransactionValidatorTrait
    for StatefulTransactionValidator<FixedBlockStateReader>
{
    async fn extract_state_nonce_and_run_validations(
        &mut self,
        executable_tx: &ExecutableTransaction,
        mempool_client: SharedMempoolClient,
    ) -> StatefulTransactionValidatorResult<Nonce> {
        let account_nonce =
            self.get_nonce_from_state(executable_tx.contract_address()).await.map_err(|e| {
                // TODO(noamsp): Fix this. Need to map the errors better.
                StarknetError::internal_with_signature_logging(
                    format!(
                        "Failed to get nonce for sender address {}",
                        executable_tx.contract_address()
                    ),
                    &executable_tx.signature(),
                    e,
                )
            })?;
        let skip_validate =
            self.run_pre_validation_checks(executable_tx, account_nonce, mempool_client).await?;
        self.run_validate_entry_point(executable_tx, skip_validate).await?;
        Ok(account_nonce)
    }
}

impl<FixedBlockStateReader: GatewayFixedBlockStateReader>
    StatefulTransactionValidator<FixedBlockStateReader>
{
    fn new(
        config: StatefulTransactionValidatorConfig,
        chain_info: ChainInfo,
        state_reader_and_contract_manager: StateReaderAndContractManager<
            Box<dyn GatewayStateReaderWithCompiledClasses>,
        >,
        gateway_fixed_block_state_reader: FixedBlockStateReader,
    ) -> Self {
        Self {
            config,
            chain_info,
            state_reader_and_contract_manager: Some(state_reader_and_contract_manager),
            gateway_fixed_block_state_reader,
        }
    }

    fn take_state_reader_and_contract_manager(
        &mut self,
    ) -> StateReaderAndContractManager<Box<dyn GatewayStateReaderWithCompiledClasses>> {
        self.state_reader_and_contract_manager.take().expect("Validator was already consumed")
    }

    async fn validate_state_preconditions(
        &self,
        executable_tx: &ExecutableTransaction,
        account_nonce: Nonce,
    ) -> StatefulTransactionValidatorResult<()> {
        self.validate_resource_bounds(executable_tx).await?;
        self.validate_nonce(executable_tx, account_nonce)?;
        Ok(())
    }

    async fn validate_resource_bounds(
        &self,
        executable_tx: &ExecutableTransaction,
    ) -> StatefulTransactionValidatorResult<()> {
        // Skip this validation during the systems bootstrap phase.
        if self.config.validate_resource_bounds {
            // TODO(Arni): getnext_l2_gas_price from the block header.
            let previous_block_l2_gas_price = self
                .gateway_fixed_block_state_reader
                .get_block_info()
                .await?
                .gas_prices
                .strk_gas_prices
                .l2_gas_price;
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
        let incoming_tx_nonce = executable_tx.nonce();

        let create_error = |message: String| {
            debug!("{message}");
            StarknetError {
                code: StarknetErrorCode::KnownErrorCode(
                    KnownStarknetErrorCode::InvalidTransactionNonce,
                ),
                message,
            }
        };

        match executable_tx {
            // Declare transactions must have the same nonce as the account nonce.
            ExecutableTransaction::Declare(_) if self.config.reject_future_declare_txs => {
                if incoming_tx_nonce != account_nonce {
                    return Err(create_error(format!(
                        "Invalid transaction nonce. Expected: nonce = {account_nonce}, got: \
                         {incoming_tx_nonce}."
                    )));
                }
            }
            // Deploy account transactions must have nonce 0.
            ExecutableTransaction::DeployAccount(_) => {
                if account_nonce != Nonce(Felt::ZERO) {
                    return Err(create_error(format!(
                        "Invalid deploy account transaction. Account is already deployed \
                         (nonce={account_nonce})."
                    )));
                }
                if incoming_tx_nonce != Nonce(Felt::ZERO) {
                    return Err(create_error(format!(
                        "Invalid transaction nonce. Expected: nonce = 0, got: {incoming_tx_nonce}."
                    )));
                }
            }
            // Other transactions must be within the allowed nonce range.
            _ => {
                let max_allowed_nonce =
                    Nonce(account_nonce.0 + Felt::from(self.config.max_allowed_nonce_gap));
                if !(account_nonce <= incoming_tx_nonce && incoming_tx_nonce <= max_allowed_nonce) {
                    return Err(create_error(format!(
                        "Invalid transaction nonce. Expected: {account_nonce} <= nonce <= \
                         {max_allowed_nonce}, got: {incoming_tx_nonce}."
                    )));
                }
            }
        }

        Ok(())
    }

    #[sequencer_latency_histogram(GATEWAY_VALIDATE_TX_LATENCY, true)]
    async fn run_validate_entry_point(
        &mut self,
        executable_tx: &ExecutableTransaction,
        skip_validate: bool,
    ) -> StatefulTransactionValidatorResult<()> {
        let only_query = false;
        let charge_fee = enforce_fee(executable_tx, only_query);
        let strict_nonce_check = false;
        let execution_flags =
            ExecutionFlags { only_query, charge_fee, validate: !skip_validate, strict_nonce_check };

        let account_tx = AccountTransaction { tx: executable_tx.clone(), execution_flags };

        // Build block context.
        let mut versioned_constants = VersionedConstants::get_versioned_constants(
            self.config.versioned_constants_overrides.clone(),
        );
        // The validation of a transaction is not affected by the casm hash migration.
        versioned_constants.enable_casm_hash_migration = false;

        let mut block_info = self.gateway_fixed_block_state_reader.get_block_info().await?;
        block_info.block_number = block_info.block_number.unchecked_next();
        let block_context = BlockContext::new(
            block_info,
            self.chain_info.clone(),
            versioned_constants,
            BouncerConfig::max(),
        );

        // Move state into the blocking task and run CPU-heavy validation.
        let state_reader_and_contract_manager = self.take_state_reader_and_contract_manager();

        let cur_span = Span::current();
        tokio::task::spawn_blocking(move || {
            cur_span.in_scope(|| {
                let state = CachedState::new(state_reader_and_contract_manager);
                let mut blockifier = BlockifierStatefulValidator::create(state, block_context);
                blockifier.validate(account_tx)
            })
        })
        .await
        .map_err(|e| StarknetError {
            code: StarknetErrorCode::UnknownErrorCode(
                "StarknetErrorCode.InternalError".to_string(),
            ),
            message: format!("Blocking task join error when running the validate entry point: {e}"),
        })?
        .map_err(|e| StarknetError {
            code: StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::ValidateFailure),
            message: e.to_string(),
        })?;
        Ok(())
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
                            "Transaction L2 gas price {tx_l2_gas_price} is below the required \
                             threshold {threshold}.",
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

    async fn get_nonce_from_state(
        &self,
        contract_address: ContractAddress,
    ) -> StatefulTransactionValidatorResult<Nonce> {
        self.gateway_fixed_block_state_reader.get_nonce(contract_address).await
    }

    async fn run_pre_validation_checks(
        &self,
        executable_tx: &ExecutableTransaction,
        account_nonce: Nonce,
        mempool_client: SharedMempoolClient,
    ) -> StatefulTransactionValidatorResult<bool> {
        self.validate_state_preconditions(executable_tx, account_nonce).await?;
        validate_by_mempool(executable_tx, account_nonce, mempool_client.clone()).await?;
        let skip_validate =
            skip_stateful_validations(executable_tx, account_nonce, mempool_client.clone()).await?;
        Ok(skip_validate)
    }
}

/// Perform transaction validation by the mempool.
async fn validate_by_mempool(
    tx: &ExecutableTransaction,
    account_nonce: Nonce,
    mempool_client: SharedMempoolClient,
) -> StatefulTransactionValidatorResult<()> {
    let validation_args = ValidationArgs::new(tx, account_nonce);
    mempool_client
        .validate_tx(validation_args)
        .await
        .map_err(|err| mempool_client_err_to_deprecated_gw_err(&tx.signature(), err))
}

/// Check if validation of an invoke transaction should be skipped due to deploy_account not being
/// processed yet. This feature is used to improve UX for users sending deploy_account + invoke at
/// once.
async fn skip_stateful_validations(
    tx: &ExecutableTransaction,
    account_nonce: Nonce,
    mempool_client: SharedMempoolClient,
) -> StatefulTransactionValidatorResult<bool> {
    if let ExecutableTransaction::Invoke(ExecutableInvokeTransaction { tx, .. }) = tx {
        // check if the transaction nonce is 1, meaning it is post deploy_account, and the
        // account nonce is zero, meaning the account was not deployed yet.
        if tx.nonce() == Nonce(Felt::ONE) && account_nonce == Nonce(Felt::ZERO) {
            let account_address = tx.sender_address();
            debug!("Checking if deploy_account transaction exists for account {account_address}.");
            // We verify that a deploy_account transaction exists for this account. It is sufficient
            // to check if the account exists in the mempool since it means that either it has a
            // deploy_account transaction or transactions with future nonces that passed
            // validations.
            return mempool_client
                .account_tx_in_pool_or_recent_block(tx.sender_address())
                .await
                .map_err(|err| mempool_client_err_to_deprecated_gw_err(&tx.signature(), err))
                .inspect(|exists| {
                    if *exists {
                        debug!("Found deploy_account transaction for account {account_address}.");
                    } else {
                        debug!(
                            "No deploy_account transaction found for account {account_address}."
                        );
                    }
                });
        }
    }

    Ok(false)
}
