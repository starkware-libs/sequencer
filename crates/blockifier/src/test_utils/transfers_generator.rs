use std::sync::Arc;
use std::time::{Duration, Instant};

use blockifier_test_utils::cairo_versions::CairoVersion;
use blockifier_test_utils::contracts::FeatureContract;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::block::BlockInfo;
use starknet_api::core::ContractAddress;
use starknet_api::executable_transaction::AccountTransaction as ApiExecutableTransaction;
use starknet_api::execution_resources::GasVector;
use starknet_api::test_utils::invoke::executable_invoke_tx;
use starknet_api::test_utils::NonceManager;
use starknet_api::transaction::constants::TRANSFER_ENTRY_POINT_NAME;
use starknet_api::transaction::fields::{Fee, ValidResourceBounds};
use starknet_api::transaction::TransactionVersion;
use starknet_api::{calldata, felt, invoke_tx_args};

use crate::blockifier::concurrent_transaction_executor::ConcurrentTransactionExecutor;
use crate::blockifier::config::{ConcurrencyConfig, TransactionExecutorConfig};
use crate::blockifier::transaction_executor::{
    BlockExecutionSummary,
    TransactionExecutor,
    TransactionExecutorError,
    DEFAULT_STACK_SIZE,
};
use crate::concurrency::worker_pool::WorkerPool;
use crate::context::{BlockContext, ChainInfo};
use crate::state::cached_state::StateMaps;
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::{RunnableCairo1, BALANCE};
use crate::transaction::account_transaction::AccountTransaction;
use crate::transaction::objects::TransactionExecutionInfo;
use crate::transaction::transaction_execution::Transaction;

const N_ACCOUNTS: u16 = 10000;
const N_TXS: usize = 1000;
const RANDOMIZATION_SEED: u64 = 0;
const CAIRO_VERSION: CairoVersion = CairoVersion::Cairo1(RunnableCairo1::Casm);
const RECIPIENT_GENERATOR_TYPE: RecipientGeneratorType = RecipientGeneratorType::RoundRobin;

pub struct TransfersGeneratorConfig {
    pub n_accounts: u16,
    pub balance: Fee,
    pub n_txs: usize,
    pub randomization_seed: u64,
    pub cairo_version: CairoVersion,
    pub recipient_generator_type: RecipientGeneratorType,
    pub concurrency_config: ConcurrencyConfig,
    pub stack_size: usize,
}

impl Default for TransfersGeneratorConfig {
    fn default() -> Self {
        let concurrency_enabled = true;
        Self {
            n_accounts: N_ACCOUNTS,
            balance: Fee(BALANCE.0 * 1000),
            n_txs: N_TXS,
            randomization_seed: RANDOMIZATION_SEED,
            cairo_version: CAIRO_VERSION,
            recipient_generator_type: RECIPIENT_GENERATOR_TYPE,
            concurrency_config: ConcurrencyConfig::create_for_testing(concurrency_enabled),
            stack_size: DEFAULT_STACK_SIZE,
        }
    }
}

#[derive(Clone, Copy)]
pub enum RecipientGeneratorType {
    Random,
    RoundRobin,
    DisjointFromSenders,
}

pub struct TransfersGenerator {
    account_contract: FeatureContract,
    account_addresses: Vec<ContractAddress>,
    block_context: BlockContext,
    chain_info: ChainInfo,
    resource_bounds: ValidResourceBounds,
    nonce_manager: NonceManager,
    sender_index: usize,
    random_recipient_generator: Option<StdRng>,
    recipient_addresses: Option<Vec<ContractAddress>>,
    config: TransfersGeneratorConfig,
}

impl TransfersGenerator {
    pub fn new(config: TransfersGeneratorConfig) -> Self {
        let account_contract = FeatureContract::AccountWithoutValidations(config.cairo_version);
        let block_context = BlockContext {
            block_info: BlockInfo::create_for_testing_with_kzg(true),
            ..BlockContext::create_for_account_testing()
        };
        let resource_bounds = ValidResourceBounds::all_bounds_from_vectors(
            &GasVector {
                l1_gas: 0_u32.into(),
                l1_data_gas: 224_u32.into(),
                l2_gas: 1_000_000_u32.into(),
            },
            &block_context.block_info.gas_prices.strk_gas_prices,
        );
        let chain_info = block_context.chain_info().clone();

        let account_addresses = (0..config.n_accounts)
            .map(|instance_id| account_contract.get_instance_address(instance_id))
            .collect::<Vec<_>>();
        let nonce_manager = NonceManager::default();
        let mut recipient_addresses = None;
        let mut random_recipient_generator = None;
        match config.recipient_generator_type {
            RecipientGeneratorType::Random => {
                // Use a random generator to get the next recipient.
                random_recipient_generator = Some(StdRng::seed_from_u64(config.randomization_seed));
            }
            RecipientGeneratorType::RoundRobin => {
                // Use the next account after the sender in the list as the recipient.
            }
            RecipientGeneratorType::DisjointFromSenders => {
                // Use a disjoint set of accounts as recipients. The index of the recipient is the
                // same as the index of the sender.
                recipient_addresses = Some(
                    (config.n_accounts..2 * config.n_accounts)
                        .map(|instance_id| account_contract.get_instance_address(instance_id))
                        .collect::<Vec<_>>(),
                );
            }
        };

        Self {
            account_contract,
            account_addresses,
            block_context,
            chain_info,
            resource_bounds,
            nonce_manager,
            sender_index: 0,
            random_recipient_generator,
            recipient_addresses,
            config,
        }
    }

    pub fn get_next_recipient(&mut self) -> ContractAddress {
        match self.config.recipient_generator_type {
            RecipientGeneratorType::Random => {
                let random_recipient_generator = self.random_recipient_generator.as_mut().unwrap();
                let recipient_index =
                    random_recipient_generator.gen_range(0..self.account_addresses.len());
                self.account_addresses[recipient_index]
            }
            RecipientGeneratorType::RoundRobin => {
                let recipient_index = (self.sender_index + 1) % self.account_addresses.len();
                self.account_addresses[recipient_index]
            }
            RecipientGeneratorType::DisjointFromSenders => {
                self.recipient_addresses.as_ref().unwrap()[self.sender_index]
            }
        }
    }

    #[allow(clippy::type_complexity)]
    fn _run_txs(
        &self,
        txs: Vec<Transaction>,
        execution_deadline: Option<Instant>,
    ) -> (
        Vec<Result<(TransactionExecutionInfo, StateMaps), TransactionExecutorError>>,
        BlockExecutionSummary,
    ) {
        let state = test_state(
            &self.chain_info,
            self.config.balance,
            &[(self.account_contract, self.config.n_accounts)],
        );
        let executor_config = TransactionExecutorConfig {
            concurrency_config: self.config.concurrency_config.clone(),
            stack_size: self.config.stack_size,
        };

        if executor_config.concurrency_config.enabled {
            let worker_pool =
                Arc::new(WorkerPool::start(&executor_config.get_worker_pool_config()));

            let mut executor = ConcurrentTransactionExecutor::new_for_testing(
                state,
                self.block_context.clone(),
                worker_pool.clone(),
                execution_deadline,
            );

            let results = executor.add_txs_and_wait(&txs);
            let block_summary = executor.close_block(results.len()).unwrap();

            drop(executor);
            Arc::try_unwrap(worker_pool)
                .expect("More than one instance of worker pool exists")
                .join();

            (results, block_summary)
        } else {
            let mut executor =
                TransactionExecutor::new(state, self.block_context.clone(), executor_config);
            let results = executor.execute_txs(&txs, execution_deadline);
            (results, executor.finalize().unwrap())
        }
    }

    /// Generates and executes transfer transactions.
    /// Returns the block summary and the execution infos.
    pub fn execute_block_of_transfers(
        &mut self,
        timeout: Option<Duration>,
    ) -> (BlockExecutionSummary, Vec<TransactionExecutionInfo>) {
        let mut txs: Vec<Transaction> = Vec::with_capacity(self.config.n_txs);
        for _ in 0..self.config.n_txs {
            let sender_address = self.account_addresses[self.sender_index];
            let recipient_address = self.get_next_recipient();
            self.sender_index = (self.sender_index + 1) % self.account_addresses.len();

            let tx = self.generate_transfer(sender_address, recipient_address);
            let account_tx = AccountTransaction::new_for_sequencing(tx);
            txs.push(Transaction::Account(account_tx));
        }
        let execution_deadline = timeout.map(|timeout| Instant::now() + timeout);
        let (results, block_summary) = self._run_txs(txs, execution_deadline);
        // Execution infos of transactions that were executed.
        let mut collected_execution_infos = Vec::<TransactionExecutionInfo>::new();
        for result in results {
            let execution_info = &result.unwrap().0;

            assert!(!execution_info.is_reverted());

            let expected_cairo_native = self.config.cairo_version.is_cairo_native();

            assert_eq!(
                execution_info.validate_call_info.as_ref().unwrap().execution.cairo_native,
                expected_cairo_native
            );
            assert_eq!(
                execution_info.execute_call_info.as_ref().unwrap().execution.cairo_native,
                expected_cairo_native
            );
            for inner_call in execution_info.execute_call_info.as_ref().unwrap().inner_calls.iter()
            {
                assert_eq!(inner_call.execution.cairo_native, expected_cairo_native);
            }
            assert_eq!(
                execution_info.fee_transfer_call_info.as_ref().unwrap().execution.cairo_native,
                expected_cairo_native
            );
            collected_execution_infos.push(execution_info.clone());
        }

        (block_summary, collected_execution_infos)

        // TODO(Avi, 01/06/2024): Run the same transactions concurrently on a new state and compare
        // the state diffs.
    }

    pub fn generate_transfer(
        &mut self,
        sender_address: ContractAddress,
        recipient_address: ContractAddress,
    ) -> ApiExecutableTransaction {
        let nonce = self.nonce_manager.next(sender_address);

        let entry_point_selector = selector_from_name(TRANSFER_ENTRY_POINT_NAME);
        let contract_address = *self.chain_info.fee_token_addresses.strk_fee_token_address.0.key();

        let execute_calldata = calldata![
            contract_address,           // Contract address.
            entry_point_selector.0,     // EP selector.
            felt!(3_u8),                // Calldata length.
            *recipient_address.0.key(), // Calldata: recipient.
            felt!(1_u8),                // Calldata: lsb amount.
            felt!(0_u8)                 // Calldata: msb amount.
        ];

        executable_invoke_tx(invoke_tx_args! {
            sender_address,
            calldata: execute_calldata,
            version: TransactionVersion::THREE,
            nonce,
            resource_bounds: self.resource_bounds,
        })
    }
}

impl Default for TransfersGenerator {
    fn default() -> Self {
        Self::new(TransfersGeneratorConfig::default())
    }
}
