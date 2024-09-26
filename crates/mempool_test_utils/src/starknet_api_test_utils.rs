use std::cell::RefCell;
use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::path::Path;
use std::rc::Rc;
use std::sync::OnceLock;

use assert_matches::assert_matches;
use blockifier::test_utils::contracts::FeatureContract;
use blockifier::test_utils::{create_trivial_calldata, CairoVersion};
use serde_json::to_string_pretty;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::executable_transaction::Transaction;
use starknet_api::rpc_transaction::{
    ContractClass,
    RpcDeclareTransactionV3,
    RpcDeployAccountTransactionV3,
    RpcInvokeTransactionV3,
    RpcTransaction,
};
use starknet_api::test_utils::NonceManager;
use starknet_api::transaction::{
    AccountDeploymentData,
    AllResourceBounds,
    Calldata,
    ContractAddressSalt,
    PaymasterData,
    ResourceBounds,
    Tip,
    TransactionSignature,
    TransactionVersion,
    ValidResourceBounds,
};
use starknet_api::{calldata, felt};
use starknet_types_core::felt::Felt;

use crate::{
    declare_tx_args,
    deploy_account_tx_args,
    get_absolute_path,
    invoke_tx_args,
    COMPILED_CLASS_HASH_OF_CONTRACT_CLASS,
    CONTRACT_CLASS_FILE,
    TEST_FILES_FOLDER,
};

pub const VALID_L1_GAS_MAX_AMOUNT: u64 = 203484;
pub const VALID_L1_GAS_MAX_PRICE_PER_UNIT: u128 = 100000000000;
pub const VALID_L2_GAS_MAX_AMOUNT: u64 = 500000;
pub const VALID_L2_GAS_MAX_PRICE_PER_UNIT: u128 = 100000000000;
pub const VALID_L1_DATA_GAS_MAX_AMOUNT: u64 = 203484;
pub const VALID_L1_DATA_GAS_MAX_PRICE_PER_UNIT: u128 = 100000000000;
pub const TEST_SENDER_ADDRESS: u128 = 0x1000;

// Utils.
pub enum TransactionType {
    Declare,
    DeployAccount,
    Invoke,
}

pub fn rpc_tx_for_testing(
    tx_type: TransactionType,
    resource_bounds: AllResourceBounds,
    calldata: Calldata,
    signature: TransactionSignature,
) -> RpcTransaction {
    match tx_type {
        TransactionType::Declare => {
            // Minimal contract class.
            let contract_class = ContractClass {
                sierra_program: vec![
                    // Sierra Version ID.
                    felt!(1_u32),
                    felt!(3_u32),
                    felt!(0_u32),
                    // Compiler version ID.
                    felt!(1_u32),
                    felt!(3_u32),
                    felt!(0_u32),
                ],
                ..Default::default()
            };
            rpc_declare_tx(declare_tx_args!(resource_bounds, signature, contract_class))
        }
        TransactionType::DeployAccount => rpc_deploy_account_tx(deploy_account_tx_args!(
            resource_bounds,
            constructor_calldata: calldata,
            signature
        )),
        TransactionType::Invoke => {
            rpc_invoke_tx(invoke_tx_args!(signature, resource_bounds, calldata))
        }
    }
}

pub const NON_EMPTY_RESOURCE_BOUNDS: ResourceBounds =
    ResourceBounds { max_amount: 1, max_price_per_unit: 1 };

// TODO(Nimrod): Delete this function.
pub fn create_resource_bounds_mapping(
    l1_resource_bounds: ResourceBounds,
    l2_resource_bounds: ResourceBounds,
    l1_data_resource_bounds: ResourceBounds,
) -> AllResourceBounds {
    AllResourceBounds {
        l1_gas: l1_resource_bounds,
        l2_gas: l2_resource_bounds,
        l1_data_gas: l1_data_resource_bounds,
    }
}

pub fn zero_resource_bounds_mapping() -> AllResourceBounds {
    AllResourceBounds::default()
}

pub fn test_resource_bounds_mapping() -> AllResourceBounds {
    create_resource_bounds_mapping(
        ResourceBounds {
            max_amount: VALID_L1_GAS_MAX_AMOUNT,
            max_price_per_unit: VALID_L1_GAS_MAX_PRICE_PER_UNIT,
        },
        ResourceBounds {
            max_amount: VALID_L2_GAS_MAX_AMOUNT,
            max_price_per_unit: VALID_L2_GAS_MAX_PRICE_PER_UNIT,
        },
        ResourceBounds {
            max_amount: VALID_L1_DATA_GAS_MAX_AMOUNT,
            max_price_per_unit: VALID_L1_DATA_GAS_MAX_PRICE_PER_UNIT,
        },
    )
}

pub fn test_valid_resource_bounds() -> ValidResourceBounds {
    ValidResourceBounds::AllResources(test_resource_bounds_mapping())
}

/// Get the contract class used for testing.
pub fn contract_class() -> ContractClass {
    env::set_current_dir(get_absolute_path(TEST_FILES_FOLDER)).expect("Couldn't set working dir.");
    let json_file_path = Path::new(CONTRACT_CLASS_FILE);
    serde_json::from_reader(File::open(json_file_path).unwrap()).unwrap()
}

/// Get the compiled class hash corresponding to the contract class used for testing.
pub fn compiled_class_hash() -> &'static CompiledClassHash {
    static COMPILED_CLASS_HASH: OnceLock<CompiledClassHash> = OnceLock::new();
    COMPILED_CLASS_HASH
        .get_or_init(|| CompiledClassHash(felt!(COMPILED_CLASS_HASH_OF_CONTRACT_CLASS)))
}

pub fn declare_tx() -> RpcTransaction {
    let contract_class = contract_class();
    let compiled_class_hash = *compiled_class_hash();

    let account_contract = FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1);
    let account_address = account_contract.get_instance_address(0);
    let mut nonce_manager = NonceManager::default();
    let nonce = nonce_manager.next(account_address);

    rpc_declare_tx(declare_tx_args!(
        signature: TransactionSignature(vec![Felt::ZERO]),
        sender_address: account_address,
        resource_bounds: test_resource_bounds_mapping(),
        nonce,
        class_hash: compiled_class_hash,
        contract_class,
    ))
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
        resource_bounds: test_resource_bounds_mapping(),
        nonce : nonce_manager.next(sender_address),
        sender_address,
        calldata: create_trivial_calldata(test_contract.get_instance_address(0))
    ))
}

pub fn executable_invoke_tx(cairo_version: CairoVersion) -> Transaction {
    let default_account = FeatureContract::AccountWithoutValidations(cairo_version);

    MultiAccountTransactionGenerator::new_for_account_contracts([default_account])
        .account_with_id(0)
        .generate_default_executable_invoke()
}

//  TODO(Yael 18/6/2024): Get a final decision from product whether to support Cairo0.
pub fn deploy_account_tx() -> RpcTransaction {
    let default_account = FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1);

    MultiAccountTransactionGenerator::new_for_account_contracts([default_account])
        .account_with_id(0)
        .generate_default_deploy_account()
}

// TODO: when moving this to Starknet API crate, move this const into a module alongside
// MultiAcconutTransactionGenerator.
type AccountId = usize;
type ContractInstanceId = u16;

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
/// use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
///
/// let mut tx_generator = MultiAccountTransactionGenerator::new(2); // Initialize with 2 accounts.
/// let account_0_tx_with_nonce_0 = tx_generator.account_with_id(0).generate_default_invoke();
/// let account_1_tx_with_nonce_0 = tx_generator.account_with_id(1).generate_default_invoke();
/// let account_0_tx_with_nonce_1 = tx_generator.account_with_id(0).generate_default_invoke();
/// ```
// Note: when moving this to starknet api crate, see if blockifier's
// [blockifier::transaction::test_utils::FaultyAccountTxCreatorArgs] can be made to use this.
pub struct MultiAccountTransactionGenerator {
    // Invariant: coupled with the nonce manager.
    account_tx_generators: Vec<AccountTransactionGenerator>,
    // Invariant: nonces managed internally thorugh `generate` API of the account transaction
    // generator.
    // Only used by single account transaction generators, but owning it here is preferable over
    // only distributing the ownership among the account generators.
    _nonce_manager: SharedNonceManager,
}

impl MultiAccountTransactionGenerator {
    pub fn new(n_accounts: usize) -> Self {
        let default_account_contract =
            FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1);
        let accounts = std::iter::repeat(default_account_contract).take(n_accounts);
        Self::new_for_account_contracts(accounts)
    }

    pub fn new_for_account_contracts(accounts: impl IntoIterator<Item = FeatureContract>) -> Self {
        let mut account_tx_generators = vec![];
        let mut account_type_to_n_instances = HashMap::new();
        let nonce_manager = SharedNonceManager::default();
        for account in accounts {
            let n_current_contract = account_type_to_n_instances.entry(account).or_insert(0);
            account_tx_generators.push(AccountTransactionGenerator {
                account,
                contract_instance_id: *n_current_contract,
                nonce_manager: nonce_manager.clone(),
            });
            *n_current_contract += 1;
        }

        Self { account_tx_generators, _nonce_manager: nonce_manager }
    }

    pub fn account_with_id(&mut self, account_id: AccountId) -> &mut AccountTransactionGenerator {
        self.account_tx_generators.get_mut(account_id).unwrap_or_else(|| {
            panic!(
                "{account_id:?} not found! This number should be an index of an account in the \
                 initialization array. "
            )
        })
    }
}

/// Manages transaction generation for a single account.
/// Supports faulty transaction generation via [AccountTransactionGenerator::generate_raw].
///
/// This struct provides methods to generate both default and fully customized transactions,
/// with room for future extensions.
///
/// TODO: add more transaction generation methods as needed.
pub struct AccountTransactionGenerator {
    account: FeatureContract,
    contract_instance_id: ContractInstanceId,
    nonce_manager: SharedNonceManager,
}

impl AccountTransactionGenerator {
    /// Generate a valid `RpcTransaction` with default parameters.
    pub fn generate_default_invoke(&mut self) -> RpcTransaction {
        let invoke_args = invoke_tx_args!(
            sender_address: self.sender_address(),
            resource_bounds: test_resource_bounds_mapping(),
            nonce: self.next_nonce(),
            calldata: create_trivial_calldata(self.test_contract_address()),
        );
        rpc_invoke_tx(invoke_args)
    }

    pub fn generate_default_executable_invoke(&mut self) -> Transaction {
        let invoke_args = starknet_api::invoke_tx_args!(
            sender_address: self.sender_address(),
            resource_bounds: test_valid_resource_bounds(),
            nonce: self.next_nonce(),
            calldata: create_trivial_calldata(self.test_contract_address()),
        );

        Transaction::Invoke(starknet_api::test_utils::invoke::executable_invoke_tx(invoke_args))
    }

    pub fn generate_default_deploy_account(&mut self) -> RpcTransaction {
        let nonce = self.next_nonce();
        assert_eq!(nonce, Nonce(Felt::ZERO));

        let deploy_account_args = deploy_account_tx_args!(
            nonce,
            class_hash: self.account.get_class_hash(),
            resource_bounds: test_resource_bounds_mapping()
        );
        rpc_deploy_account_tx(deploy_account_args)
    }

    // TODO: support more contracts, instead of this hardcoded type.
    pub fn test_contract_address(&mut self) -> ContractAddress {
        let cairo_version = self.account.cairo_version();
        FeatureContract::TestContract(cairo_version).get_instance_address(0)
    }

    /// Generates an `RpcTransaction` with fully custom parameters.
    ///
    /// Caller must manually handle bumping nonce and fetching the correct sender address via
    /// [AccountTransactionGenerator::next_nonce] and [AccountTransactionGenerator::sender_address].
    /// See [AccountTransactionGenerator::generate_default_invoke] to have these filled up by
    /// default.
    ///
    /// Note: This is a best effort attempt to make the API more useful; amend or add new methods
    /// as needed.
    pub fn generate_raw(&mut self, invoke_tx_args: InvokeTxArgs) -> RpcTransaction {
        rpc_invoke_tx(invoke_tx_args)
    }

    pub fn sender_address(&mut self) -> ContractAddress {
        self.account.get_instance_address(self.contract_instance_id)
    }

    /// Retrieves the nonce for the current account, and __increments__ it internally.
    pub fn next_nonce(&mut self) -> Nonce {
        let sender_address = self.sender_address();
        self.nonce_manager.borrow_mut().next(sender_address)
    }
}

/// Adds state to the feature contract struct, so that its _account_ variants can generate a single
/// address, thus allowing future transactions generated for the account to share the same address.
#[derive(Debug, Clone)]
pub struct FeatureAccount {
    pub class_hash: ClassHash,
    pub account: FeatureContract,
    deployment_state: InitializationState,
}

impl FeatureAccount {
    pub fn new(account: FeatureContract) -> Self {
        assert_matches!(
            account,
            FeatureContract::AccountWithLongValidate(_)
                | FeatureContract::AccountWithoutValidations(_)
                | FeatureContract::FaultyAccount(_),
            "{account:?} is not an account"
        );

        Self {
            class_hash: account.get_class_hash(),
            account,
            deployment_state: InitializationState::default(),
        }
    }

    pub fn build(&mut self, deploy_account_tx: &RpcTransaction) {
        assert_matches!(
            deploy_account_tx,
            RpcTransaction::DeployAccount(_),
            "An account must be initialized with a deploy account transaction"
        );

        match self.deployment_state {
            InitializationState::Uninitialized => {
                let address = deploy_account_tx.calculate_sender_address().unwrap();
                self.deployment_state = InitializationState::Initialized(address);
            }
            InitializationState::Initialized(_) => panic!("Account is already initialized"),
        }
    }

    pub fn sender_address(&self) -> ContractAddress {
        match self.deployment_state {
            InitializationState::Initialized(address) => address,
            InitializationState::Uninitialized => panic!("Uninitialized account"),
        }
    }

    // Use for special case testing accounts that don't have an explicit deploy account transaction.
    pub fn new_with_custom_address(account: FeatureContract, address: ContractAddress) -> Self {
        Self {
            account,
            deployment_state: InitializationState::Initialized(address),
            class_hash: account.get_class_hash(),
        }
    }
}

#[derive(Clone, Debug, Default)]
enum InitializationState {
    #[default]
    Uninitialized,
    Initialized(ContractAddress),
}

// TODO(Ayelet, 28/5/2025): Try unifying the macros.
// TODO(Ayelet, 28/5/2025): Consider moving the macros StarkNet API.
#[macro_export]
macro_rules! invoke_tx_args {
    ($($field:ident $(: $value:expr)?),* $(,)?) => {
        $crate::starknet_api_test_utils::InvokeTxArgs {
            $($field $(: $value)?,)*
            ..Default::default()
        }
    };
    ($($field:ident $(: $value:expr)?),* , ..$defaults:expr) => {
        $crate::starknet_api_test_utils::InvokeTxArgs {
            $($field $(: $value)?,)*
            ..$defaults
        }
    };
}

#[macro_export]
macro_rules! deploy_account_tx_args {
    ($($field:ident $(: $value:expr)?),* $(,)?) => {
        $crate::starknet_api_test_utils::DeployAccountTxArgs {
            $($field $(: $value)?,)*
            ..Default::default()
        }
    };
    ($($field:ident $(: $value:expr)?),* , ..$defaults:expr) => {
        $crate::starknet_api_test_utils::DeployAccountTxArgs {
            $($field $(: $value)?,)*
            ..$defaults
        }
    };
}

#[macro_export]
macro_rules! declare_tx_args {
    ($($field:ident $(: $value:expr)?),* $(,)?) => {
        $crate::starknet_api_test_utils::DeclareTxArgs {
            $($field $(: $value)?,)*
            ..Default::default()
        }
    };
    ($($field:ident $(: $value:expr)?),* , ..$defaults:expr) => {
        $crate::starknet_api_test_utils::DeclareTxArgs {
            $($field $(: $value)?,)*
            ..$defaults
        }
    };
}

#[derive(Clone)]
pub struct InvokeTxArgs {
    pub signature: TransactionSignature,
    pub sender_address: ContractAddress,
    pub calldata: Calldata,
    pub version: TransactionVersion,
    pub resource_bounds: AllResourceBounds,
    pub tip: Tip,
    pub nonce_data_availability_mode: DataAvailabilityMode,
    pub fee_data_availability_mode: DataAvailabilityMode,
    pub paymaster_data: PaymasterData,
    pub account_deployment_data: AccountDeploymentData,
    pub nonce: Nonce,
}

impl Default for InvokeTxArgs {
    fn default() -> Self {
        InvokeTxArgs {
            signature: TransactionSignature::default(),
            sender_address: ContractAddress::default(),
            calldata: calldata![],
            version: TransactionVersion::THREE,
            resource_bounds: zero_resource_bounds_mapping(),
            tip: Tip::default(),
            nonce_data_availability_mode: DataAvailabilityMode::L1,
            fee_data_availability_mode: DataAvailabilityMode::L1,
            paymaster_data: PaymasterData::default(),
            account_deployment_data: AccountDeploymentData::default(),
            nonce: Nonce::default(),
        }
    }
}

#[derive(Clone)]
pub struct DeployAccountTxArgs {
    pub signature: TransactionSignature,
    pub version: TransactionVersion,
    pub resource_bounds: AllResourceBounds,
    pub tip: Tip,
    pub nonce_data_availability_mode: DataAvailabilityMode,
    pub fee_data_availability_mode: DataAvailabilityMode,
    pub paymaster_data: PaymasterData,
    pub nonce: Nonce,
    pub class_hash: ClassHash,
    pub contract_address_salt: ContractAddressSalt,
    pub constructor_calldata: Calldata,
}

impl Default for DeployAccountTxArgs {
    fn default() -> Self {
        DeployAccountTxArgs {
            signature: TransactionSignature::default(),
            version: TransactionVersion::THREE,
            resource_bounds: zero_resource_bounds_mapping(),
            tip: Tip::default(),
            nonce_data_availability_mode: DataAvailabilityMode::L1,
            fee_data_availability_mode: DataAvailabilityMode::L1,
            paymaster_data: PaymasterData::default(),
            nonce: Nonce::default(),
            class_hash: ClassHash::default(),
            contract_address_salt: ContractAddressSalt::default(),
            constructor_calldata: Calldata::default(),
        }
    }
}

#[derive(Clone)]
pub struct DeclareTxArgs {
    pub signature: TransactionSignature,
    pub sender_address: ContractAddress,
    pub version: TransactionVersion,
    pub resource_bounds: AllResourceBounds,
    pub tip: Tip,
    pub nonce_data_availability_mode: DataAvailabilityMode,
    pub fee_data_availability_mode: DataAvailabilityMode,
    pub paymaster_data: PaymasterData,
    pub account_deployment_data: AccountDeploymentData,
    pub nonce: Nonce,
    pub class_hash: CompiledClassHash,
    pub contract_class: ContractClass,
}

impl Default for DeclareTxArgs {
    fn default() -> Self {
        Self {
            signature: TransactionSignature::default(),
            sender_address: ContractAddress::default(),
            version: TransactionVersion::THREE,
            resource_bounds: zero_resource_bounds_mapping(),
            tip: Tip::default(),
            nonce_data_availability_mode: DataAvailabilityMode::L1,
            fee_data_availability_mode: DataAvailabilityMode::L1,
            paymaster_data: PaymasterData::default(),
            account_deployment_data: AccountDeploymentData::default(),
            nonce: Nonce::default(),
            class_hash: CompiledClassHash::default(),
            contract_class: ContractClass::default(),
        }
    }
}

pub fn rpc_invoke_tx(invoke_args: InvokeTxArgs) -> RpcTransaction {
    if invoke_args.version != TransactionVersion::THREE {
        panic!("Unsupported transaction version: {:?}.", invoke_args.version);
    }

    starknet_api::rpc_transaction::RpcTransaction::Invoke(
        starknet_api::rpc_transaction::RpcInvokeTransaction::V3(RpcInvokeTransactionV3 {
            resource_bounds: invoke_args.resource_bounds,
            tip: invoke_args.tip,
            calldata: invoke_args.calldata,
            sender_address: invoke_args.sender_address,
            nonce: invoke_args.nonce,
            signature: invoke_args.signature,
            nonce_data_availability_mode: invoke_args.nonce_data_availability_mode,
            fee_data_availability_mode: invoke_args.fee_data_availability_mode,
            paymaster_data: invoke_args.paymaster_data,
            account_deployment_data: invoke_args.account_deployment_data,
        }),
    )
}

pub fn rpc_deploy_account_tx(deploy_tx_args: DeployAccountTxArgs) -> RpcTransaction {
    if deploy_tx_args.version != TransactionVersion::THREE {
        panic!("Unsupported transaction version: {:?}.", deploy_tx_args.version);
    }

    starknet_api::rpc_transaction::RpcTransaction::DeployAccount(
        starknet_api::rpc_transaction::RpcDeployAccountTransaction::V3(
            RpcDeployAccountTransactionV3 {
                resource_bounds: deploy_tx_args.resource_bounds,
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

pub fn rpc_declare_tx(declare_tx_args: DeclareTxArgs) -> RpcTransaction {
    if declare_tx_args.version != TransactionVersion::THREE {
        panic!("Unsupported transaction version: {:?}.", declare_tx_args.version);
    }

    starknet_api::rpc_transaction::RpcTransaction::Declare(
        starknet_api::rpc_transaction::RpcDeclareTransaction::V3(RpcDeclareTransactionV3 {
            contract_class: declare_tx_args.contract_class,
            signature: declare_tx_args.signature,
            sender_address: declare_tx_args.sender_address,
            resource_bounds: declare_tx_args.resource_bounds,
            tip: declare_tx_args.tip,
            nonce_data_availability_mode: declare_tx_args.nonce_data_availability_mode,
            fee_data_availability_mode: declare_tx_args.fee_data_availability_mode,
            paymaster_data: declare_tx_args.paymaster_data,
            account_deployment_data: declare_tx_args.account_deployment_data,
            nonce: declare_tx_args.nonce,
            compiled_class_hash: declare_tx_args.class_hash,
        }),
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
