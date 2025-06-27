#![allow(non_local_definitions)]

use blockifier::blockifier::stateful_validator::{StatefulValidator, StatefulValidatorResult};
use blockifier::blockifier_versioned_constants::VersionedConstants;
use blockifier::bouncer::BouncerConfig;
use blockifier::context::BlockContext;
use blockifier::state::cached_state::CachedState;
use blockifier::transaction::account_transaction::AccountTransaction;
use blockifier::transaction::objects::TransactionInfoCreator;
use pyo3::{pyclass, pymethods, PyAny};
use starknet_api::core::Nonce;
use starknet_api::executable_transaction::TransactionType;
use starknet_api::transaction::TransactionHash;
use starknet_types_core::felt::Felt;

use crate::errors::NativeBlockifierResult;
use crate::py_block_executor::PyOsConfig;
use crate::py_objects::PyVersionedConstantsOverrides;
use crate::py_state_diff::PyBlockInfo;
use crate::py_transaction::{py_account_tx, PyClassInfo, PY_TX_PARSING_ERR};
use crate::py_utils::PyFelt;
use crate::state_readers::py_state_reader::PyStateReader;

#[pyclass]
pub struct PyValidator {
    pub stateful_validator: StatefulValidator<PyStateReader>,
    pub max_nonce_for_validation_skip: Nonce,
}

#[pymethods]
impl PyValidator {
    #[new]
    #[pyo3(signature = (os_config, state_reader_proxy, next_block_info, max_nonce_for_validation_skip, py_versioned_constants_overrides))]
    pub fn create(
        os_config: PyOsConfig,
        state_reader_proxy: &PyAny,
        next_block_info: PyBlockInfo,
        max_nonce_for_validation_skip: PyFelt,
        py_versioned_constants_overrides: PyVersionedConstantsOverrides,
    ) -> NativeBlockifierResult<Self> {
        // Create the state.
        let state_reader = PyStateReader::new(state_reader_proxy);
        let state = CachedState::new(state_reader);

        // Create the block context.
        let versioned_constants =
            VersionedConstants::get_versioned_constants(py_versioned_constants_overrides.into());
        let block_context = BlockContext::new(
            next_block_info.try_into().expect("Failed to convert block info."),
            os_config.into_chain_info(),
            versioned_constants,
            BouncerConfig::max(),
        );

        // Create the stateful validator.
        let max_nonce_for_validation_skip = Nonce(max_nonce_for_validation_skip.0);
        let stateful_validator = StatefulValidator::create(state, block_context);

        Ok(Self { stateful_validator, max_nonce_for_validation_skip })
    }

    // Transaction Execution API.

    #[pyo3(signature = (tx, optional_py_class_info, deploy_account_tx_hash))]
    pub fn perform_validations(
        &mut self,
        tx: &PyAny,
        optional_py_class_info: Option<PyClassInfo>,
        deploy_account_tx_hash: Option<PyFelt>,
    ) -> NativeBlockifierResult<()> {
        let mut account_tx = py_account_tx(tx, optional_py_class_info).expect(PY_TX_PARSING_ERR);
        let deploy_account_tx_hash = deploy_account_tx_hash.map(|hash| TransactionHash(hash.0));

        // We check if the transaction should be skipped due to the deploy account not being
        // processed.
        let validate = self
            .should_run_stateful_validations(&account_tx, deploy_account_tx_hash)
            .map_err(Box::new)?;

        account_tx.execution_flags.validate = validate;
        account_tx.execution_flags.strict_nonce_check = false;
        self.stateful_validator.perform_validations(account_tx).map_err(Box::new)?;

        Ok(())
    }
}

impl PyValidator {
    // Returns whether the transaction should be statefully validated.
    // If the DeployAccount transaction of the account was submitted but not processed yet, it
    // should be skipped for subsequent transactions for a better user experience. (they will
    // otherwise fail solely because the deploy account hasn't been processed yet).
    pub fn should_run_stateful_validations(
        &mut self,
        account_tx: &AccountTransaction,
        deploy_account_tx_hash: Option<TransactionHash>,
    ) -> StatefulValidatorResult<bool> {
        if account_tx.tx_type() != TransactionType::InvokeFunction {
            return Ok(true);
        }
        let tx_info = account_tx.create_tx_info();
        let nonce = self.stateful_validator.get_nonce(tx_info.sender_address())?;

        let deploy_account_not_processed =
            deploy_account_tx_hash.is_some() && nonce == Nonce(Felt::ZERO);
        let tx_nonce = tx_info.nonce();
        let is_post_deploy_nonce = Nonce(Felt::ONE) <= tx_nonce;
        let nonce_small_enough_to_qualify_for_validation_skip =
            tx_nonce <= self.max_nonce_for_validation_skip;

        let skip_validate = deploy_account_not_processed
            && is_post_deploy_nonce
            && nonce_small_enough_to_qualify_for_validation_skip;

        Ok(!skip_validate)
    }
}
