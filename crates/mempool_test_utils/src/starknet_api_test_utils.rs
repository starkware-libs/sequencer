use std::cell::RefCell;
use std::env;
use std::fs::File;
use std::path::Path;
use std::rc::Rc;
use std::sync::LazyLock;

use assert_matches::assert_matches;
use blockifier::test_utils::contracts::FeatureContract;
use blockifier::test_utils::{create_trivial_calldata, CairoVersion};
use infra_utils::path::resolve_project_relative_path;
use pretty_assertions::assert_ne;
use serde_json::to_string_pretty;
use starknet_api::block::GasPrice;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::executable_transaction::AccountTransaction;
use starknet_api::execution_resources::GasAmount;
use starknet_api::rpc_transaction::{ContractClass, RpcDeployAccountTransactionV3, RpcTransaction};
use starknet_api::test_utils::declare::rpc_declare_tx;
use starknet_api::test_utils::deploy_account::DeployAccountTxArgs;
use starknet_api::test_utils::invoke::{rpc_invoke_tx, InvokeTxArgs};
use starknet_api::test_utils::NonceManager;
use starknet_api::transaction::fields::{
    AllResourceBounds,
    ContractAddressSalt,
    ResourceBounds,
    Tip,
    TransactionSignature,
    ValidResourceBounds,
};
use starknet_api::transaction::TransactionVersion;
use starknet_api::{declare_tx_args, deploy_account_tx_args, felt, invoke_tx_args, nonce};
use starknet_types_core::felt::Felt;

use crate::{COMPILED_CLASS_HASH_OF_CONTRACT_CLASS, CONTRACT_CLASS_FILE, TEST_FILES_FOLDER};

pub const VALID_L1_GAS_MAX_AMOUNT: u64 = 203484;
pub const VALID_L1_GAS_MAX_PRICE_PER_UNIT: u128 = 100000000000;
pub const VALID_L2_GAS_MAX_AMOUNT: u64 = 500000;
pub const VALID_L2_GAS_MAX_PRICE_PER_UNIT: u128 = 100000000000;
pub const VALID_L1_DATA_GAS_MAX_AMOUNT: u64 = 203484;
pub const VALID_L1_DATA_GAS_MAX_PRICE_PER_UNIT: u128 = 100000000000;

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
pub fn contract_class() -> ContractClass {
    env::set_current_dir(resolve_project_relative_path(TEST_FILES_FOLDER).unwrap())
        .expect("Couldn't set working dir.");
    let json_file_path = Path::new(CONTRACT_CLASS_FILE);
    serde_json::from_reader(File::open(json_file_path).unwrap()).unwrap()
}

pub static COMPILED_CLASS_HASH: LazyLock<CompiledClassHash> =
    LazyLock::new(|| CompiledClassHash(felt!(COMPILED_CLASS_HASH_OF_CONTRACT_CLASS)));

pub fn declare_tx() -> RpcTransaction {
    let contract_class = contract_class();
    let compiled_class_hash = *COMPILED_CLASS_HASH;

    let account_contract = FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1);
    let account_address = account_contract.get_instance_address(0);
    let mut nonce_manager = NonceManager::default();
    let nonce = nonce_manager.next(account_address);

    rpc_declare_tx(
        declare_tx_args!(
            signature: TransactionSignature(vec![Felt::ZERO]),
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

    let mut tx_generator = MultiAccountTransactionGenerator::new();
    tx_generator.register_account(default_account);
    tx_generator.account_with_id(0).generate_executable_invoke()
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

// TODO: when moving this to Starknet API crate, move this const into a module alongside
// MultiAcconutTransactionGenerator.
pub type AccountId = usize;

type SharedNonceManager = Rc<RefCell<NonceManager>>;

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
/// use blockifier::test_utils::contracts::FeatureContract;
/// use blockifier::test_utils::CairoVersion;
/// use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
///
/// let mut tx_generator = MultiAccountTransactionGenerator::new();
/// let some_account_type = FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1);
/// // Initialize multiple accounts, these can be any account type in `FeatureContract`.
/// tx_generator.register_account_for_flow_test(some_account_type.clone());
/// tx_generator.register_account_for_flow_test(some_account_type);
///
/// let account_0_tx_with_nonce_0 = tx_generator.account_with_id(0).generate_invoke_with_tip(1);
/// let account_1_tx_with_nonce_0 = tx_generator.account_with_id(1).generate_invoke_with_tip(3);
/// let account_0_tx_with_nonce_1 = tx_generator.account_with_id(0).generate_invoke_with_tip(1);
/// ```
// Note: when moving this to starknet api crate, see if blockifier's
// [blockifier::transaction::test_utils::FaultyAccountTxCreatorArgs] can be made to use this.
#[derive(Default)]
pub struct MultiAccountTransactionGenerator {
    // Invariant: coupled with the nonce manager.
    account_tx_generators: Vec<AccountTransactionGenerator>,
    // Invariant: nonces managed internally thorugh `generate` API of the account transaction
    // generator.
    nonce_manager: SharedNonceManager,
}

impl MultiAccountTransactionGenerator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_account(&mut self, account_contract: FeatureContract) -> RpcTransaction {
        let new_account_id = self.account_tx_generators.len();
        let (account_tx_generator, default_deploy_account_tx) = AccountTransactionGenerator::new(
            new_account_id,
            account_contract,
            self.nonce_manager.clone(),
        );
        self.account_tx_generators.push(account_tx_generator);

        default_deploy_account_tx
    }

    pub fn account_with_id(&mut self, account_id: AccountId) -> &mut AccountTransactionGenerator {
        self.account_tx_generators.get_mut(account_id).unwrap_or_else(|| {
            panic!(
                "{account_id:?} not found! This number should be an index of an account in the \
                 initialization array. "
            )
        })
    }

    // TODO(deploy_account_support): once we support deploy account in tests, remove this method and
    // only use new_account_default in tests. In particular, deploy account txs must be then sent to
    // the GW via the add tx endpoint just like other txs.
    pub fn register_account_for_flow_test(&mut self, account_contract: FeatureContract) {
        self.register_account(account_contract);
    }

    pub fn accounts(&self) -> Vec<Contract> {
        self.account_tx_generators.iter().map(|tx_gen| &tx_gen.account).copied().collect()
    }
}

/// Manages transaction generation for a single account.
/// Supports faulty transaction generation via [AccountTransactionGenerator::generate_raw_invoke].
///
/// This struct provides methods to generate both default and fully customized transactions,
/// with room for future extensions.
///
/// TODO: add more transaction generation methods as needed.
#[derive(Debug)]
pub struct AccountTransactionGenerator {
    account: Contract,
    nonce_manager: SharedNonceManager,
}

impl AccountTransactionGenerator {
    /// Generate a valid `RpcTransaction` with default parameters.
    pub fn generate_invoke_with_tip(&mut self, tip: u64) -> RpcTransaction {
        let nonce = self.next_nonce();
        assert_ne!(
            nonce,
            nonce!(0),
            "Cannot invoke on behalf of an undeployed account: the first transaction of every \
             account must be a deploy account transaction."
        );
        let invoke_args = invoke_tx_args!(
            nonce,
            tip : Tip(tip),
            sender_address: self.sender_address(),
            resource_bounds: test_valid_resource_bounds(),
            calldata: create_trivial_calldata(self.sender_address()),
        );
        rpc_invoke_tx(invoke_args)
    }

    pub fn generate_executable_invoke(&mut self) -> AccountTransaction {
        let nonce = self.next_nonce();
        assert_ne!(
            nonce,
            nonce!(0),
            "Cannot invoke on behalf of an undeployed account: the first transaction of every \
             account must be a deploy account transaction."
        );

        let invoke_args = invoke_tx_args!(
            sender_address: self.sender_address(),
            resource_bounds: test_valid_resource_bounds(),
            nonce,
            calldata: create_trivial_calldata(self.sender_address()),
        );

        AccountTransaction::Invoke(starknet_api::test_utils::invoke::executable_invoke_tx(
            invoke_args,
        ))
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

    pub fn sender_address(&mut self) -> ContractAddress {
        self.account.sender_address
    }

    /// Retrieves the nonce for the current account, and __increments__ it internally.
    pub fn next_nonce(&mut self) -> Nonce {
        let sender_address = self.sender_address();
        self.nonce_manager.borrow_mut().next(sender_address)
    }

    /// Private constructor, since only the multi-account transaction generator should create this
    /// struct.
    // TODO: add a version that doesn't rely on the default deploy account constructor, but takes
    // deploy account args.
    fn new(
        account_id: usize,
        account: FeatureContract,
        nonce_manager: SharedNonceManager,
    ) -> (Self, RpcTransaction) {
        let contract_address_salt = ContractAddressSalt(account_id.into());
        // A deploy account transaction must be created now in order to affix an address to it.
        // If this doesn't happen now it'll be difficult to fund the account during test setup.
        let default_deploy_account_tx =
            generate_deploy_account_with_salt(&account, contract_address_salt);

        let mut account_tx_generator = Self {
            account: Contract::new_for_account(account, &default_deploy_account_tx),
            nonce_manager,
        };
        // Bump the account nonce after transaction creation.
        account_tx_generator.next_nonce();

        (account_tx_generator, default_deploy_account_tx)
    }
}

/// Extends (account) feature contracts with a fixed sender address.
/// The address is calculated from a deploy account transaction and cached.
// Note: feature contracts have their own address generating method, but it a mocked address and is
// not related to an actual deploy account transaction, which is the way real account addresses are
// calculated.
#[derive(Clone, Copy, Debug)]
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

pub fn rpc_deploy_account_tx(deploy_tx_args: DeployAccountTxArgs) -> RpcTransaction {
    if deploy_tx_args.version != TransactionVersion::THREE {
        panic!("Unsupported transaction version: {:?}.", deploy_tx_args.version);
    }

    let ValidResourceBounds::AllResources(resource_bounds) = deploy_tx_args.resource_bounds else {
        panic!("Unsupported resource bounds type: {:?}.", deploy_tx_args.resource_bounds)
    };

    starknet_api::rpc_transaction::RpcTransaction::DeployAccount(
        starknet_api::rpc_transaction::RpcDeployAccountTransaction::V3(
            RpcDeployAccountTransactionV3 {
                resource_bounds,
                tip: deploy_tx_args.tip,
                contract_address_salt: deploy_tx_args.contract_address_salt,
                class_hash: deploy_tx_args.class_hash,
                constructor_calldata: deploy_tx_args.constructor_calldata,
                nonce: deploy_tx_args.nonce,
                signature: deploy_tx_args.signature,
                nonce_data_availability_mode: deploy_tx_args.nonce_data_availability_mode,
                fee_data_availability_mode: deploy_tx_args.fee_data_availability_mode,
                paymaster_data: deploy_tx_args.paymaster_data,
            },
        ),
    )
}

pub fn rpc_tx_to_json(tx: &RpcTransaction) -> String {
    let mut tx_json = serde_json::to_value(tx)
        .unwrap_or_else(|tx| panic!("Failed to serialize transaction: {tx:?}"));

    // Add type and version manually
    let type_string = match tx {
        RpcTransaction::Declare(_) => "DECLARE",
        RpcTransaction::DeployAccount(_) => "DEPLOY_ACCOUNT",
        RpcTransaction::Invoke(_) => "INVOKE",
    };

    tx_json
        .as_object_mut()
        .unwrap()
        .extend([("type".to_string(), type_string.into()), ("version".to_string(), "0x3".into())]);

    // Serialize back to pretty JSON string
    to_string_pretty(&tx_json).expect("Failed to serialize transaction")
}
