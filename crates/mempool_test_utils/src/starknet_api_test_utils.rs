use std::cell::RefCell;
use std::fs::File;
use std::rc::Rc;
use std::sync::LazyLock;

use apollo_infra_utils::path::resolve_project_relative_path;
use assert_matches::assert_matches;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::calldata::create_trivial_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use papyrus_base_layer::ethereum_base_layer_contract::L1ToL2MessageArgs;
use papyrus_base_layer::test_utils::DEFAULT_ANVIL_L1_ACCOUNT_ADDRESS;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::block::GasPrice;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::executable_transaction::{AccountTransaction, DeclareTransaction};
use starknet_api::execution_resources::GasAmount;
use starknet_api::hash::StarkHash;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::state::SierraContractClass;
use starknet_api::test_utils::declare::rpc_declare_tx;
use starknet_api::test_utils::deploy_account::rpc_deploy_account_tx;
use starknet_api::test_utils::invoke::{rpc_invoke_tx, InvokeTxArgs};
use starknet_api::test_utils::{NonceManager, TEST_ERC20_CONTRACT_ADDRESS2};
use starknet_api::transaction::constants::TRANSFER_ENTRY_POINT_NAME;
use starknet_api::transaction::fields::{
    AllResourceBounds,
    ContractAddressSalt,
    Fee,
    ResourceBounds,
    Tip,
    TransactionSignature,
    ValidResourceBounds,
};
use starknet_api::transaction::L1HandlerTransaction;
use starknet_api::{
    calldata,
    declare_tx_args,
    deploy_account_tx_args,
    felt,
    invoke_tx_args,
    nonce,
};
use starknet_types_core::felt::Felt;

use crate::{COMPILED_CLASS_HASH_OF_CONTRACT_CLASS, CONTRACT_CLASS_FILE, TEST_FILES_FOLDER};

pub const VALID_L1_GAS_MAX_AMOUNT: u64 = 203484;
pub const VALID_L1_GAS_MAX_PRICE_PER_UNIT: u128 = 100000000000000;
pub const VALID_L2_GAS_MAX_AMOUNT: u64 = 500000 * 200000; // Enough to declare the test class.
pub const VALID_L2_GAS_MAX_PRICE_PER_UNIT: u128 = 100000000000000;
pub const VALID_L1_DATA_GAS_MAX_AMOUNT: u64 = 203484;
pub const VALID_L1_DATA_GAS_MAX_PRICE_PER_UNIT: u128 = 100000000000000;
#[allow(clippy::as_conversions)]
pub const VALID_ACCOUNT_BALANCE: Fee =
    Fee(VALID_L2_GAS_MAX_AMOUNT as u128 * VALID_L2_GAS_MAX_PRICE_PER_UNIT * 1000);

// Utils.

// TODO(Noam): Merge this into test_valid_resource_bounds
pub fn test_resource_bounds_mapping() -> AllResourceBounds {
    AllResourceBounds {
        l1_gas: ResourceBounds {
            max_amount: GasAmount(VALID_L1_GAS_MAX_AMOUNT),
            max_price_per_unit: GasPrice(VALID_L1_GAS_MAX_PRICE_PER_UNIT),
        },
        l2_gas: ResourceBounds {
            max_amount: GasAmount(VALID_L2_GAS_MAX_AMOUNT),
            max_price_per_unit: GasPrice(VALID_L2_GAS_MAX_PRICE_PER_UNIT),
        },
        l1_data_gas: ResourceBounds {
            max_amount: GasAmount(VALID_L1_DATA_GAS_MAX_AMOUNT),
            max_price_per_unit: GasPrice(VALID_L1_DATA_GAS_MAX_PRICE_PER_UNIT),
        },
    }
}

pub fn test_valid_resource_bounds() -> ValidResourceBounds {
    ValidResourceBounds::AllResources(test_resource_bounds_mapping())
}

/// Get the contract class used for testing.
pub fn contract_class() -> SierraContractClass {
    let test_files_folder_path = resolve_project_relative_path(TEST_FILES_FOLDER).unwrap();
    let json_file_path = test_files_folder_path.join(CONTRACT_CLASS_FILE);
    serde_json::from_reader(File::open(json_file_path).unwrap()).unwrap()
}

pub static COMPILED_CLASS_HASH: LazyLock<CompiledClassHash> =
    LazyLock::new(|| CompiledClassHash(felt!(COMPILED_CLASS_HASH_OF_CONTRACT_CLASS)));

pub fn declare_tx() -> RpcTransaction {
    let contract_class = contract_class();
    let compiled_class_hash = *COMPILED_CLASS_HASH;

    let account_contract =
        FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let account_address = account_contract.get_instance_address(0);
    let mut nonce_manager = NonceManager::default();
    let nonce = nonce_manager.next(account_address);

    rpc_declare_tx(
        declare_tx_args!(
            signature: TransactionSignature(vec![Felt::ZERO].into()),
            sender_address: account_address,
            resource_bounds: test_valid_resource_bounds(),
            nonce,
            compiled_class_hash: compiled_class_hash
        ),
        contract_class,
    )
}

/// Convenience method for generating a single invoke transaction with trivial fields.
/// For multiple, nonce-incrementing transactions under a single account address, use the
/// transaction generator..
pub fn invoke_tx(cairo_version: CairoVersion) -> RpcTransaction {
    let test_contract = FeatureContract::TestContract(cairo_version);
    let account_contract = FeatureContract::AccountWithoutValidations(cairo_version);
    let sender_address = account_contract.get_instance_address(0);
    let mut nonce_manager = NonceManager::default();

    rpc_invoke_tx(invoke_tx_args!(
        resource_bounds: test_valid_resource_bounds(),
        nonce : nonce_manager.next(sender_address),
        sender_address,
        calldata: create_trivial_calldata(test_contract.get_instance_address(0))
    ))
}

pub fn executable_invoke_tx(cairo_version: CairoVersion) -> AccountTransaction {
    let default_account = FeatureContract::AccountWithoutValidations(cairo_version);
    let default_test_contract =
        FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));

    let mut tx_generator = MultiAccountTransactionGenerator::new();
    tx_generator.register_deployed_account(default_account, default_test_contract);
    tx_generator.account_with_id_mut(0).generate_executable_invoke()
}

pub fn deploy_account_tx() -> RpcTransaction {
    generate_deploy_account_with_salt(
        &FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1(RunnableCairo1::Casm)),
        ContractAddressSalt(0_u64.into()),
    )
}

pub fn generate_deploy_account_with_salt(
    account: &FeatureContract,
    contract_address_salt: ContractAddressSalt,
) -> RpcTransaction {
    let deploy_account_args = deploy_account_tx_args!(
        class_hash: account.get_class_hash(),
        resource_bounds: test_valid_resource_bounds(),
        contract_address_salt
    );

    rpc_deploy_account_tx(deploy_account_args)
}

// TODO(Gilad): when moving this to Starknet API crate, move this const into a module alongside
// MultiAccountTransactionGenerator.
pub type AccountId = usize;

type SharedNonceManager = Rc<RefCell<NonceManager>>;

struct L1HandlerTransactionGenerator {
    // The L1 nonce for the next created L1 handler transaction.
    l1_tx_nonce: u64,
}

impl Default for L1HandlerTransactionGenerator {
    /// The Anvil instance is spawned with a nonce of 1 for the account [Self::L1_ACCOUNT_ADDRESS].
    fn default() -> Self {
        Self { l1_tx_nonce: 1 }
    }
}

impl L1HandlerTransactionGenerator {
    const L1_ACCOUNT_ADDRESS: StarkHash = DEFAULT_ANVIL_L1_ACCOUNT_ADDRESS;

    /// Creates an L1 handler transaction calling the "l1_handler_set_value" entry point in
    /// [TestContract](FeatureContract::TestContract).
    fn create_l1_to_l2_message_args(&mut self) -> L1ToL2MessageArgs {
        let l1_tx_nonce = self.l1_tx_nonce;
        self.l1_tx_nonce += 1;
        // TODO(Arni): Get test contract from test setup.
        let test_contract =
            FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));

        let l1_handler_tx = L1HandlerTransaction {
            contract_address: test_contract.get_instance_address(0),
            // TODO(Arni): Consider saving this value as a lazy constant.
            entry_point_selector: selector_from_name("l1_handler_set_value"),
            calldata: calldata![
                Self::L1_ACCOUNT_ADDRESS,
                // Arbitrary key and value.
                felt!("0x876"), // key
                felt!("0x44")   // value
            ],
            ..Default::default()
        };

        L1ToL2MessageArgs { tx: l1_handler_tx, l1_tx_nonce }
    }

    fn n_generated_txs(&self) -> u64 {
        self.l1_tx_nonce - 1
    }
}

// TODO(Yair): Separate MultiAccountTransactionGenerator to phases:
// 1. Setup phase - register erc20 contract and initially deployed account with some balance
//    (produce the state diff that represents the initial state so it can be used in the test).
// 2. Execution phase - generate transactions.

// TODO(Yair): Add optional StateReader and assert that the state supports each operation (e.g.
// nonce).

/// Manages transaction generation for multiple pre-funded accounts, internally bumping nonces
/// as needed.
///
/// **Currently supports:**
/// - Single contract type
/// - Only supports invokes, which are all a trivial method in the contract type.
///
/// # Example
///
/// ```
/// use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
/// use blockifier_test_utils::contracts::FeatureContract;
/// use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
/// use starknet_api::transaction::fields::ContractAddressSalt;
///
/// let mut tx_generator = MultiAccountTransactionGenerator::new();
/// let some_account_type =
///     FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1(RunnableCairo1::Casm));
/// let default_test_contract =
///     FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));
/// // Initialize multiple accounts, these can be any account type in `FeatureContract`.
/// tx_generator
///     .register_deployed_account(some_account_type.clone(), default_test_contract.clone());
/// tx_generator
///     .register_deployed_account(some_account_type.clone(), default_test_contract.clone());
///
/// let account_0_tx_with_nonce_0 = tx_generator.account_with_id_mut(0).generate_invoke_with_tip(1);
/// let account_1_tx_with_nonce_0 = tx_generator.account_with_id_mut(1).generate_invoke_with_tip(3);
/// let account_0_tx_with_nonce_1 = tx_generator.account_with_id_mut(0).generate_invoke_with_tip(1);
///
/// // Initialize an undeployed account.
/// let salt = ContractAddressSalt(123_u64.into());
/// tx_generator.register_undeployed_account(
///     some_account_type,
///     default_test_contract.clone(),
///     salt,
/// );
/// let undeployed_account = tx_generator.account_with_id(2).account;
/// // Generate a transfer to fund the undeployed account.
/// let transfer_tx = tx_generator.account_with_id_mut(0).generate_transfer(&undeployed_account);
/// // Generate a deploy account transaction for the undeployed account.
/// let deploy_account_tx = tx_generator.account_with_id_mut(2).generate_deploy_account();
/// ```
// Note: when moving this to starknet api crate, see if blockifier's
// [blockifier::transaction::test_utils::FaultyAccountTxCreatorArgs] can be made to use this.
#[derive(Default)]
pub struct MultiAccountTransactionGenerator {
    // Invariant: coupled with the nonce manager.
    account_tx_generators: Vec<AccountTransactionGenerator>,
    // Invariant: nonces managed internally through `generate` API of the account transaction
    // generator.
    nonce_manager: SharedNonceManager,
    l1_handler_tx_generator: L1HandlerTransactionGenerator,
}

impl MultiAccountTransactionGenerator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> Self {
        let nonce_manager = Rc::new(RefCell::new((*self.nonce_manager.borrow()).clone()));
        let account_tx_generators = self
            .account_tx_generators
            .iter()
            .map(|tx_gen| AccountTransactionGenerator {
                account: tx_gen.account,
                test_contract: tx_gen.test_contract,
                nonce_manager: nonce_manager.clone(),
                contract_address_salt: tx_gen.contract_address_salt,
            })
            .collect();
        let l1_handler_tx_generator =
            L1HandlerTransactionGenerator { l1_tx_nonce: self.l1_handler_tx_generator.l1_tx_nonce };

        Self { account_tx_generators, nonce_manager, l1_handler_tx_generator }
    }

    /// Registers a new account with the given contract, assuming it is already deployed.
    /// The test_contract provides the targets for invoke transactions generated by the account.
    /// Note: the state should reflect that the account is already deployed.
    pub fn register_deployed_account(
        &mut self,
        account_contract: FeatureContract,
        test_contract: FeatureContract,
    ) -> AccountId {
        let new_account_id = self.account_tx_generators.len();
        let salt = ContractAddressSalt(new_account_id.into());
        let (account_tx_generator, _default_deploy_account_tx) = AccountTransactionGenerator::new(
            account_contract,
            test_contract,
            self.nonce_manager.clone(),
            salt,
            true,
        );
        self.account_tx_generators.push(account_tx_generator);
        new_account_id
    }

    /// Registers a new undeployed account with the given contract.
    /// The test_contract provides the targets for invoke transactions generated by the account.
    pub fn register_undeployed_account(
        &mut self,
        account_contract: FeatureContract,
        test_contract: FeatureContract,
        contract_address_salt: ContractAddressSalt,
    ) -> AccountId {
        let new_account_id = self.account_tx_generators.len();
        let (account_tx_generator, _default_deploy_account_tx) = AccountTransactionGenerator::new(
            account_contract,
            test_contract,
            self.nonce_manager.clone(),
            contract_address_salt,
            false,
        );
        self.account_tx_generators.push(account_tx_generator);
        new_account_id
    }

    pub fn account_with_id_mut(
        &mut self,
        account_id: AccountId,
    ) -> &mut AccountTransactionGenerator {
        self.account_tx_generators.get_mut(account_id).unwrap_or_else(|| {
            panic!(
                "{account_id:?} not found! This number should be an index of an account in the \
                 initialization array. "
            )
        })
    }

    pub fn account_with_id(&self, account_id: AccountId) -> &AccountTransactionGenerator {
        self.account_tx_generators.get(account_id).unwrap_or_else(|| {
            panic!(
                "{account_id:?} not found! This number should be an index of an account in the \
                 initialization array. "
            )
        })
    }

    pub fn accounts(&self) -> &[AccountTransactionGenerator] {
        self.account_tx_generators.as_slice()
    }

    pub fn account_tx_generators(&mut self) -> &mut Vec<AccountTransactionGenerator> {
        &mut self.account_tx_generators
    }

    pub fn deployed_accounts(&self) -> Vec<Contract> {
        self.account_tx_generators
            .iter()
            .filter_map(|tx_gen| if tx_gen.is_deployed() { Some(&tx_gen.account) } else { None })
            .copied()
            .collect()
    }

    pub fn undeployed_accounts(&self) -> Vec<Contract> {
        self.account_tx_generators
            .iter()
            .filter_map(|tx_gen| if !tx_gen.is_deployed() { Some(&tx_gen.account) } else { None })
            .copied()
            .collect()
    }

    pub fn create_l1_to_l2_message_args(&mut self) -> L1ToL2MessageArgs {
        self.l1_handler_tx_generator.create_l1_to_l2_message_args()
    }

    pub fn n_l1_txs(&self) -> usize {
        self.l1_handler_tx_generator
            .n_generated_txs()
            .try_into()
            .expect("Failed to convert nonce to usize")
    }
}

/// Manages transaction generation for a single account.
/// Supports faulty transaction generation via [AccountTransactionGenerator::generate_raw_invoke].
///
/// This struct provides methods to generate both default and fully customized transactions,
/// with room for future extensions.
///
/// TODO(Gilad): add more transaction generation methods as needed.
#[derive(Clone, Debug)]
pub struct AccountTransactionGenerator {
    pub account: Contract,
    test_contract: FeatureContract,
    nonce_manager: SharedNonceManager,
    contract_address_salt: ContractAddressSalt,
}

impl AccountTransactionGenerator {
    pub fn is_deployed(&self) -> bool {
        self.nonce_manager.borrow().get(self.sender_address()) != nonce!(0)
    }

    /// Generate a valid `RpcTransaction` with default parameters.
    pub fn generate_invoke_with_tip(&mut self, tip: u64) -> RpcTransaction {
        assert!(
            self.is_deployed(),
            "Cannot invoke on behalf of an undeployed account: the first transaction of every \
             account must be a deploy account transaction."
        );
        let nonce = self.next_nonce();

        let invoke_args = invoke_tx_args!(
            nonce,
            tip : Tip(tip),
            sender_address: self.sender_address(),
            resource_bounds: test_valid_resource_bounds(),
            calldata: create_trivial_calldata(self.test_contract_address()),
        );
        rpc_invoke_tx(invoke_args)
    }

    pub fn generate_executable_invoke(&mut self) -> AccountTransaction {
        assert!(
            self.is_deployed(),
            "Cannot invoke on behalf of an undeployed account: the first transaction of every \
             account must be a deploy account transaction."
        );
        let nonce = self.next_nonce();
        let invoke_args = invoke_tx_args!(
            sender_address: self.sender_address(),
            resource_bounds: test_valid_resource_bounds(),
            nonce,
            calldata: create_trivial_calldata(self.test_contract_address()),
        );

        starknet_api::test_utils::invoke::executable_invoke_tx(invoke_args)
    }

    /// Generates an `RpcTransaction` with fully custom parameters.
    ///
    /// Caller must manually handle bumping nonce and fetching the correct sender address via
    /// [AccountTransactionGenerator::next_nonce] and [AccountTransactionGenerator::sender_address].
    /// See [AccountTransactionGenerator::generate_invoke_with_tip] to have these filled up by
    /// default.
    ///
    /// Note: This is a best effort attempt to make the API more useful; amend or add new methods
    /// as needed.
    pub fn generate_raw_invoke(&mut self, invoke_tx_args: InvokeTxArgs) -> RpcTransaction {
        rpc_invoke_tx(invoke_tx_args)
    }

    pub fn generate_transfer(&mut self, recipient: &Contract) -> RpcTransaction {
        let nonce = self.next_nonce();
        let entry_point_selector = selector_from_name(TRANSFER_ENTRY_POINT_NAME);
        let erc20_address = felt!(TEST_ERC20_CONTRACT_ADDRESS2);

        let calldata = calldata![
            erc20_address,                   // Contract address.
            entry_point_selector.0,          // EP selector.
            felt!(3_u8),                     // Calldata length.
            *recipient.sender_address.key(), // Calldata: recipient.
            felt!(1_u8),                     // Calldata: lsb amount.
            felt!(0_u8)                      // Calldata: msb amount.
        ];

        let invoke_args = invoke_tx_args!(
            sender_address: self.sender_address(),
            resource_bounds: test_valid_resource_bounds(),
            nonce,
            calldata
        );

        rpc_invoke_tx(invoke_args)
    }

    pub fn generate_deploy_account(&mut self) -> RpcTransaction {
        assert!(
            !self.is_deployed(),
            "Cannot deploy an already deployed account: the first transaction of every account \
             must be a deploy account transaction."
        );
        let nonce = self.next_nonce();
        assert_eq!(nonce, nonce!(0), "The deploy account tx should have nonce 0.");
        let deploy_account_args = deploy_account_tx_args!(
            class_hash: self.account.class_hash(),
            resource_bounds: test_valid_resource_bounds(),
            contract_address_salt: ContractAddressSalt(self.contract_address_salt.0)
        );
        rpc_deploy_account_tx(deploy_account_args)
    }

    pub fn generate_declare(&mut self) -> RpcTransaction {
        let nonce = self.next_nonce();
        let declare_args = declare_tx_args!(
            signature: TransactionSignature(vec![Felt::ZERO].into()),
            sender_address: self.sender_address(),
            resource_bounds: test_valid_resource_bounds(),
            nonce,
            compiled_class_hash: *COMPILED_CLASS_HASH,
        );
        let contract_class = contract_class();
        rpc_declare_tx(declare_args, contract_class)
    }

    pub fn sender_address(&self) -> ContractAddress {
        self.account.sender_address
    }

    fn test_contract_address(&self) -> ContractAddress {
        self.test_contract.get_instance_address(0)
    }

    /// Retrieves the nonce for the current account, and __increments__ it internally.
    pub fn next_nonce(&mut self) -> Nonce {
        let sender_address = self.sender_address();
        self.nonce_manager.borrow_mut().next(sender_address)
    }

    /// Retrieves the nonce for the current account.
    pub fn get_nonce(&self) -> Nonce {
        let sender_address = self.sender_address();
        self.nonce_manager.borrow().get(sender_address)
    }

    /// Private constructor, since only the multi-account transaction generator should create this
    /// struct.
    // TODO(Gilad): add a version that doesn't rely on the default deploy account constructor, but
    // takes deploy account args.
    fn new(
        account: FeatureContract,
        test_contract: FeatureContract,
        nonce_manager: SharedNonceManager,
        contract_address_salt: ContractAddressSalt,
        is_deployed: bool,
    ) -> (Self, RpcTransaction) {
        // A deploy account transaction must be created now in order to affix an address to it.
        // If this doesn't happen now it'll be difficult to fund the account during test setup.
        let default_deploy_account_tx =
            generate_deploy_account_with_salt(&account, contract_address_salt);

        let mut account_tx_generator = Self {
            account: Contract::new_for_account(account, &default_deploy_account_tx),
            test_contract,
            nonce_manager,
            contract_address_salt,
        };
        if is_deployed {
            // Bump the account nonce after transaction creation.
            account_tx_generator.next_nonce();
        }

        (account_tx_generator, default_deploy_account_tx)
    }
}

/// Generate a declare transaction for initial bootstrapping phase (no fees).
pub fn generate_bootstrap_declare() -> RpcTransaction {
    let bootstrap_declare_args = declare_tx_args!(
        signature: TransactionSignature::default(),
        sender_address: DeclareTransaction::bootstrap_address(),
        resource_bounds: ValidResourceBounds::create_for_testing_no_fee_enforcement(),
        nonce: Nonce(Felt::ZERO),
        compiled_class_hash: *COMPILED_CLASS_HASH,
    );
    rpc_declare_tx(bootstrap_declare_args, contract_class())
}

/// Extends (account) feature contracts with a fixed sender address.
/// The address is calculated from a deploy account transaction and cached.
// Note: feature contracts have their own address generating method, but it a mocked address and is
// not related to an actual deploy account transaction, which is the way real account addresses are
// calculated.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contract {
    pub contract: FeatureContract,
    pub sender_address: ContractAddress,
}

impl Contract {
    pub fn class_hash(&self) -> ClassHash {
        self.contract.get_class_hash()
    }

    pub fn cairo_version(&self) -> CairoVersion {
        self.contract.cairo_version()
    }

    pub fn sierra(&self) -> SierraContractClass {
        self.contract.get_sierra()
    }

    pub fn raw_class(&self) -> String {
        self.contract.get_raw_class()
    }

    fn new_for_account(account: FeatureContract, deploy_account_tx: &RpcTransaction) -> Self {
        assert_matches!(
            deploy_account_tx,
            RpcTransaction::DeployAccount(_),
            "An account must be initialized with a deploy account transaction"
        );
        assert_matches!(
            account,
            FeatureContract::AccountWithLongValidate(_)
                | FeatureContract::AccountWithoutValidations(_)
                | FeatureContract::FaultyAccount(_),
            "{account:?} is not an account"
        );

        Self {
            contract: account,
            sender_address: deploy_account_tx.calculate_sender_address().unwrap(),
        }
    }
}
