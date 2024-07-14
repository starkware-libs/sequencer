use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::path::Path;

use assert_matches::assert_matches;
use blockifier::test_utils::contracts::FeatureContract;
use blockifier::test_utils::{create_trivial_calldata, CairoVersion, NonceManager};
use serde_json::to_string_pretty;
use starknet_api::core::{
    calculate_contract_address, ClassHash, CompiledClassHash, ContractAddress, Nonce,
};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::rpc_transaction::{
    ContractClass, RPCDeclareTransactionV3, RPCDeployAccountTransaction,
    RPCDeployAccountTransactionV3, RPCInvokeTransactionV3, RPCTransaction, ResourceBoundsMapping,
};
use starknet_api::transaction::{
    AccountDeploymentData, Calldata, ContractAddressSalt, PaymasterData, ResourceBounds, Tip,
    TransactionSignature, TransactionVersion,
};
use starknet_api::{calldata, felt};
use starknet_types_core::felt::Felt;

use crate::{
    declare_tx_args, deploy_account_tx_args, get_absolute_path, invoke_tx_args,
    COMPILED_CLASS_HASH_OF_CONTRACT_CLASS, CONTRACT_CLASS_FILE, TEST_FILES_FOLDER,
};

pub const VALID_L1_GAS_MAX_AMOUNT: u64 = 203483;
pub const VALID_L1_GAS_MAX_PRICE_PER_UNIT: u128 = 100000000000;
pub const TEST_SENDER_ADDRESS: u128 = 0x1000;

// Utils.
pub enum TransactionType {
    Declare,
    DeployAccount,
    Invoke,
}

pub fn external_tx_for_testing(
    tx_type: TransactionType,
    resource_bounds: ResourceBoundsMapping,
    calldata: Calldata,
    signature: TransactionSignature,
) -> RPCTransaction {
    match tx_type {
        TransactionType::Declare => {
            // Minimal contract class.
            let contract_class = ContractClass {
                sierra_program: vec![felt!(1_u32), felt!(3_u32), felt!(0_u32)],
                ..Default::default()
            };
            external_declare_tx(declare_tx_args!(resource_bounds, signature, contract_class))
        }
        TransactionType::DeployAccount => external_deploy_account_tx(deploy_account_tx_args!(
            resource_bounds,
            constructor_calldata: calldata,
            signature
        )),
        TransactionType::Invoke => {
            external_invoke_tx(invoke_tx_args!(signature, resource_bounds, calldata))
        }
    }
}

pub const NON_EMPTY_RESOURCE_BOUNDS: ResourceBounds =
    ResourceBounds { max_amount: 1, max_price_per_unit: 1 };

pub fn create_resource_bounds_mapping(
    l1_resource_bounds: ResourceBounds,
    l2_resource_bounds: ResourceBounds,
) -> ResourceBoundsMapping {
    ResourceBoundsMapping { l1_gas: l1_resource_bounds, l2_gas: l2_resource_bounds }
}

pub fn zero_resource_bounds_mapping() -> ResourceBoundsMapping {
    create_resource_bounds_mapping(ResourceBounds::default(), ResourceBounds::default())
}

pub fn executable_resource_bounds_mapping() -> ResourceBoundsMapping {
    create_resource_bounds_mapping(
        ResourceBounds {
            max_amount: VALID_L1_GAS_MAX_AMOUNT,
            max_price_per_unit: VALID_L1_GAS_MAX_PRICE_PER_UNIT,
        },
        ResourceBounds::default(),
    )
}

pub fn declare_tx() -> RPCTransaction {
    env::set_current_dir(get_absolute_path(TEST_FILES_FOLDER)).expect("Couldn't set working dir.");
    let json_file_path = Path::new(CONTRACT_CLASS_FILE);
    let contract_class = serde_json::from_reader(File::open(json_file_path).unwrap()).unwrap();
    let compiled_class_hash = CompiledClassHash(felt!(COMPILED_CLASS_HASH_OF_CONTRACT_CLASS));

    let account_contract = FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1);
    let account_address = account_contract.get_instance_address(0);
    let mut nonce_manager = NonceManager::default();
    let nonce = nonce_manager.next(account_address);

    external_declare_tx(declare_tx_args!(
        signature: TransactionSignature(vec![Felt::ZERO]),
        sender_address: account_address,
        resource_bounds: executable_resource_bounds_mapping(),
        nonce,
        class_hash: compiled_class_hash,
        contract_class,
    ))
}

// Convenience method for generating a single invoke transaction with trivial fields.
// For multiple, nonce-incrementing transactions, use the transaction generator directly.
pub fn invoke_tx(cairo_version: CairoVersion) -> RPCTransaction {
    let default_account = FeatureContract::AccountWithoutValidations(cairo_version);

    MultiAccountTransactionGenerator::new_for_account_contracts([default_account])
        .account_with_id(0)
        .generate_default_invoke()
}

//  TODO(Yael 18/6/2024): Get a final decision from product whether to support Cairo0.
pub fn deploy_account_tx() -> RPCTransaction {
    let default_account = FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1);

    MultiAccountTransactionGenerator::new_for_account_contracts([default_account])
        .account_with_id(0)
        .generate_default_deploy_account()
}

// TODO: when moving this to Starknet API crate, move this const into a module alongside
// MultiAcconutTransactionGenerator.
type AccountId = u16;

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
    // Invariant: coupled with nonce_manager.
    account_contracts: HashMap<AccountId, FeatureContract>,
    // Invariant: nonces managed internally thorugh `generate` API.
    nonce_manager: NonceManager,
}

impl MultiAccountTransactionGenerator {
    pub fn new(n_accounts: usize) -> Self {
        let default_account_contract =
            FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1);
        let accounts = std::iter::repeat(default_account_contract).take(n_accounts);
        Self::new_for_account_contracts(accounts)
    }

    pub fn new_for_account_contracts(accounts: impl IntoIterator<Item = FeatureContract>) -> Self {
        let enumerated_accounts = (0..).zip(accounts);
        let account_contracts = enumerated_accounts.collect();

        Self { account_contracts, nonce_manager: NonceManager::default() }
    }

    pub fn account_with_id(&mut self, account_id: AccountId) -> AccountTransactionGenerator<'_> {
        AccountTransactionGenerator { account_id, generator: self }
    }
}

/// Manages transaction generation for a single account.
/// Supports faulty transaction generation via [AccountTransactionGenerator::generate_raw].
///
/// This struct provides methods to generate both default and fully customized transactions,
/// with room for future extensions.
///
/// TODO: add more transaction generation methods as needed.
pub struct AccountTransactionGenerator<'a> {
    account_id: AccountId,
    generator: &'a mut MultiAccountTransactionGenerator,
}

impl<'a> AccountTransactionGenerator<'a> {
    /// Generate a valid `RPCTransaction` with default parameters.
    pub fn generate_default_invoke(&mut self) -> RPCTransaction {
        let invoke_args = invoke_tx_args!(
            sender_address: self.sender_address(),
            resource_bounds: executable_resource_bounds_mapping(),
            nonce: self.next_nonce(),
            calldata: create_trivial_calldata(self.test_contract_address()),
        );
        external_invoke_tx(invoke_args)
    }

    pub fn generate_default_deploy_account(&mut self) -> RPCTransaction {
        let nonce = self.next_nonce();
        assert_eq!(nonce, Nonce(Felt::ZERO));

        let deploy_account_args = deploy_account_tx_args!(
            nonce,
            class_hash: self.generator.account_contracts[&self.account_id].get_class_hash(),
            resource_bounds: executable_resource_bounds_mapping()
        );
        external_deploy_account_tx(deploy_account_args)
    }

    // TODO: support more contracts, instead of this hardcoded type.
    pub fn test_contract_address(&mut self) -> ContractAddress {
        let cairo_version = self.generator.account_contracts[&self.account_id].cairo_version();
        FeatureContract::TestContract(cairo_version).get_instance_address(self.account_id)
    }

    /// Generates an `RPCTransaction` with fully custom parameters.
    ///
    /// Caller must manually handle bumping nonce and fetching the correct sender address via
    /// [AccountTransactionGenerator::nonce] and [AccountTransactionGenerator::sender_address].
    /// See [AccountTransactionGenerator::generate_default] to have these filled up by default.
    ///
    /// Note: This is a best effort attempt to make the API more useful; amend or add new methods
    /// as needed.
    pub fn generate_raw(&mut self, invoke_tx_args: InvokeTxArgs) -> RPCTransaction {
        external_invoke_tx(invoke_tx_args)
    }

    pub fn sender_address(&mut self) -> ContractAddress {
        let account_id = self.account_id;
        self.generator.account_contracts[&account_id].get_instance_address(account_id)
    }

    /// Retrieves the nonce for the current account, and __increments__ it internally.
    pub fn next_nonce(&mut self) -> Nonce {
        let sender_address = self.sender_address();
        self.generator.nonce_manager.next(sender_address)
    }
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
    pub resource_bounds: ResourceBoundsMapping,
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
    pub resource_bounds: ResourceBoundsMapping,
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
    pub resource_bounds: ResourceBoundsMapping,
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

pub fn external_invoke_tx(invoke_args: InvokeTxArgs) -> RPCTransaction {
    if invoke_args.version != TransactionVersion::THREE {
        panic!("Unsupported transaction version: {:?}.", invoke_args.version);
    }

    starknet_api::rpc_transaction::RPCTransaction::Invoke(
        starknet_api::rpc_transaction::RPCInvokeTransaction::V3(RPCInvokeTransactionV3 {
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

pub fn external_deploy_account_tx(deploy_tx_args: DeployAccountTxArgs) -> RPCTransaction {
    if deploy_tx_args.version != TransactionVersion::THREE {
        panic!("Unsupported transaction version: {:?}.", deploy_tx_args.version);
    }

    starknet_api::rpc_transaction::RPCTransaction::DeployAccount(
        starknet_api::rpc_transaction::RPCDeployAccountTransaction::V3(
            RPCDeployAccountTransactionV3 {
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

pub fn external_declare_tx(declare_tx_args: DeclareTxArgs) -> RPCTransaction {
    if declare_tx_args.version != TransactionVersion::THREE {
        panic!("Unsupported transaction version: {:?}.", declare_tx_args.version);
    }

    starknet_api::rpc_transaction::RPCTransaction::Declare(
        starknet_api::rpc_transaction::RPCDeclareTransaction::V3(RPCDeclareTransactionV3 {
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

pub fn external_tx_to_json(tx: &RPCTransaction) -> String {
    let mut tx_json = serde_json::to_value(tx)
        .unwrap_or_else(|tx| panic!("Failed to serialize transaction: {tx:?}"));

    // Add type and version manually
    let type_string = match tx {
        RPCTransaction::Declare(_) => "DECLARE",
        RPCTransaction::DeployAccount(_) => "DEPLOY_ACCOUNT",
        RPCTransaction::Invoke(_) => "INVOKE",
    };

    tx_json
        .as_object_mut()
        .unwrap()
        .extend([("type".to_string(), type_string.into()), ("version".to_string(), "0x3".into())]);

    // Serialize back to pretty JSON string
    to_string_pretty(&tx_json).expect("Failed to serialize transaction")
}

pub fn deployed_account_contract_address(deploy_tx: &RPCTransaction) -> ContractAddress {
    let tx = assert_matches!(
        deploy_tx,
        RPCTransaction::DeployAccount(RPCDeployAccountTransaction::V3(tx)) => tx
    );
    calculate_contract_address(
        tx.contract_address_salt,
        tx.class_hash,
        &tx.constructor_calldata,
        ContractAddress::default(),
    )
    .unwrap()
}
