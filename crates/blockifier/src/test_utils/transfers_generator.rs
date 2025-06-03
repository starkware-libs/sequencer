use std::time::{Duration, Instant};

use blockifier_test_utils::cairo_versions::CairoVersion;
use blockifier_test_utils::contracts::FeatureContract;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::core::ContractAddress;
use starknet_api::executable_transaction::AccountTransaction as ApiExecutableTransaction;
use starknet_api::test_utils::invoke::executable_invoke_tx;
use starknet_api::test_utils::NonceManager;
use starknet_api::transaction::constants::TRANSFER_ENTRY_POINT_NAME;
use starknet_api::transaction::fields::Fee;
use starknet_api::transaction::TransactionVersion;
use starknet_api::{calldata, felt, invoke_tx_args};
use starknet_types_core::felt::Felt;

use crate::blockifier::config::{ConcurrencyConfig, TransactionExecutorConfig};
use crate::blockifier::transaction_executor::{
    BlockExecutionSummary,
    TransactionExecutor,
    DEFAULT_STACK_SIZE,
};
use crate::context::{BlockContext, ChainInfo};
use crate::test_utils::dict_state_reader::DictStateReader;
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::{RunnableCairo1, BALANCE, MAX_FEE};
use crate::transaction::account_transaction::AccountTransaction;
use crate::transaction::objects::TransactionExecutionInfo;
use crate::transaction::transaction_execution::Transaction;
const N_ACCOUNTS: u16 = 10000;
const N_TXS: usize = 1000;
const RANDOMIZATION_SEED: u64 = 0;
const CAIRO_VERSION: CairoVersion = CairoVersion::Cairo1(RunnableCairo1::Casm);
const TRANSACTION_VERSION: TransactionVersion = TransactionVersion(Felt::THREE);
const RECIPIENT_GENERATOR_TYPE: RecipientGeneratorType = RecipientGeneratorType::RoundRobin;

pub struct TransfersGeneratorConfig {
    pub n_accounts: u16,
    pub balance: Fee,
    pub max_fee: Fee,
    pub n_txs: usize,
    pub randomization_seed: u64,
    pub cairo_version: CairoVersion,
    pub tx_version: TransactionVersion,
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
            max_fee: MAX_FEE,
            n_txs: N_TXS,
            randomization_seed: RANDOMIZATION_SEED,
            cairo_version: CAIRO_VERSION,
            tx_version: TRANSACTION_VERSION,
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
    account_addresses: Vec<ContractAddress>,
    chain_info: ChainInfo,
    executor: TransactionExecutor<DictStateReader>,
    nonce_manager: NonceManager,
    sender_index: usize,
    random_recipient_generator: Option<StdRng>,
    recipient_addresses: Option<Vec<ContractAddress>>,
    config: TransfersGeneratorConfig,
    // Execution infos of transactions that were executed during the lifetime of this generator.
    collected_execution_infos: Vec<TransactionExecutionInfo>,
}

impl TransfersGenerator {
    pub fn new(config: TransfersGeneratorConfig) -> Self {
        let account_contract = FeatureContract::AccountWithoutValidations(config.cairo_version);
        let block_context = BlockContext::create_for_account_testing();
        let chain_info = block_context.chain_info().clone();
        let state =
            test_state(&chain_info, config.balance, &[(account_contract, config.n_accounts)]);
        let executor_config = TransactionExecutorConfig {
            concurrency_config: config.concurrency_config.clone(),
            stack_size: config.stack_size,
        };
        let executor = TransactionExecutor::new(state, block_context, executor_config);
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
            account_addresses,
            chain_info,
            executor,
            nonce_manager,
            sender_index: 0,
            random_recipient_generator,
            recipient_addresses,
            config,
            collected_execution_infos: vec![],
        }
    }

    /// Finalizes the transaction executor and returns the ongoing transaction execution infos
    /// and the block execution summary.
    pub fn finalize(mut self) -> (Vec<TransactionExecutionInfo>, BlockExecutionSummary) {
        (self.collected_execution_infos, self.executor.finalize().unwrap())
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

    /// Generates and executes transfer transactions.
    /// Returns the number of transactions executed.
    pub fn execute_transfers(&mut self, timeout: Option<Duration>) -> usize {
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
        let results = self.executor.execute_txs(&txs, execution_deadline);
        let n_results = results.len();
        for result in results {
            let execution_info = result.unwrap().0;
            assert!(!execution_info.is_reverted());
            self.collected_execution_infos.push(execution_info);
        }

        n_results
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
        let contract_address = if self.config.tx_version == TransactionVersion::ONE {
            *self.chain_info.fee_token_addresses.eth_fee_token_address.0.key()
        } else if self.config.tx_version == TransactionVersion::THREE {
            *self.chain_info.fee_token_addresses.strk_fee_token_address.0.key()
        } else {
            panic!("Unsupported transaction version: {:?}", self.config.tx_version)
        };

        let execute_calldata = calldata![
            contract_address,           // Contract address.
            entry_point_selector.0,     // EP selector.
            felt!(3_u8),                // Calldata length.
            *recipient_address.0.key(), // Calldata: recipient.
            felt!(1_u8),                // Calldata: lsb amount.
            felt!(0_u8)                 // Calldata: msb amount.
        ];

        executable_invoke_tx(invoke_tx_args! {
            max_fee: self.config.max_fee,
            sender_address,
            calldata: execute_calldata,
            version: self.config.tx_version,
            nonce,
        })
    }
}

impl Default for TransfersGenerator {
    fn default() -> Self {
        Self::new(TransfersGeneratorConfig::default())
    }
}
