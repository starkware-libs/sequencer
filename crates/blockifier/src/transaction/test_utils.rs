use blockifier_test_utils::cairo_versions::CairoVersion;
use blockifier_test_utils::calldata::create_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use rstest::fixture;
use starknet_api::abi::abi_utils::get_fee_token_var_address;
use starknet_api::block::{FeeType, GasPrice};
use starknet_api::contract_class::{ClassInfo, ContractClass, SierraVersion};
use starknet_api::core::{ClassHash, ContractAddress, Nonce};
use starknet_api::executable_transaction::TransactionType;
use starknet_api::execution_resources::{GasAmount, GasVector};
use starknet_api::test_utils::declare::executable_declare_tx;
use starknet_api::test_utils::deploy_account::{
    create_executable_deploy_account_tx_and_update_nonce,
    DeployAccountTxArgs,
};
use starknet_api::test_utils::invoke::{executable_invoke_tx, InvokeTxArgs};
use starknet_api::test_utils::{
    NonceManager,
    DEFAULT_L1_DATA_GAS_MAX_AMOUNT,
    DEFAULT_L1_GAS_AMOUNT,
    DEFAULT_L2_GAS_MAX_AMOUNT,
    DEFAULT_STRK_L1_DATA_GAS_PRICE,
    DEFAULT_STRK_L1_GAS_PRICE,
    DEFAULT_STRK_L2_GAS_PRICE,
    MAX_FEE,
};
use starknet_api::transaction::fields::{
    AllResourceBounds,
    ContractAddressSalt,
    Fee,
    GasVectorComputationMode,
    ResourceBounds,
    TransactionSignature,
    ValidResourceBounds,
};
use starknet_api::transaction::{constants, TransactionVersion};
use starknet_api::{calldata, declare_tx_args, deploy_account_tx_args, felt, invoke_tx_args};
use starknet_types_core::felt::Felt;
use strum::IntoEnumIterator;

use crate::blockifier_versioned_constants::VersionedConstants;
use crate::context::{BlockContext, ChainInfo};
use crate::state::cached_state::CachedState;
use crate::state::state_api::State;
use crate::test_utils::contracts::FeatureContractTrait;
use crate::test_utils::dict_state_reader::DictStateReader;
use crate::test_utils::initial_test_state::test_state;
use crate::test_utils::BALANCE;
use crate::transaction::account_transaction::{AccountTransaction, ExecutionFlags};
use crate::transaction::objects::{TransactionExecutionInfo, TransactionExecutionResult};
use crate::transaction::transactions::ExecutableTransaction;

// Corresponding constants to the ones in faulty_account.
pub const VALID: u64 = 0;
pub const INVALID: u64 = 1;
pub const CALL_CONTRACT: u64 = 2;
pub const GET_BLOCK_HASH: u64 = 3;
pub const GET_EXECUTION_INFO: u64 = 4;
pub const GET_BLOCK_NUMBER: u64 = 5;
pub const GET_BLOCK_TIMESTAMP: u64 = 6;
pub const GET_SEQUENCER_ADDRESS: u64 = 7;
pub const STORAGE_WRITE: u64 = 8;

/// Test fixtures.

#[fixture]
pub fn block_context() -> BlockContext {
    BlockContext::create_for_account_testing()
}

#[fixture]
pub fn versioned_constants(block_context: BlockContext) -> VersionedConstants {
    block_context.versioned_constants().clone()
}

#[fixture]
pub fn max_fee() -> Fee {
    MAX_FEE
}

// TODO(Amos, 1/10/2024): Delete this fixture and use `create_resource_bounds`
#[fixture]
pub fn default_l1_resource_bounds() -> ValidResourceBounds {
    create_resource_bounds(&GasVectorComputationMode::NoL2Gas)
}

#[fixture]
pub fn default_all_resource_bounds() -> ValidResourceBounds {
    create_resource_bounds(&GasVectorComputationMode::All)
}

pub fn create_resource_bounds(computation_mode: &GasVectorComputationMode) -> ValidResourceBounds {
    match computation_mode {
        GasVectorComputationMode::NoL2Gas => {
            l1_resource_bounds(DEFAULT_L1_GAS_AMOUNT, DEFAULT_STRK_L1_GAS_PRICE.into())
        }
        GasVectorComputationMode::All => create_gas_amount_bounds_with_default_price(GasVector {
            l1_gas: DEFAULT_L1_GAS_AMOUNT,
            l1_data_gas: DEFAULT_L1_DATA_GAS_MAX_AMOUNT,
            l2_gas: DEFAULT_L2_GAS_MAX_AMOUNT,
        }),
    }
}

pub fn create_gas_amount_bounds_with_default_price(
    GasVector { l1_gas, l1_data_gas, l2_gas }: GasVector,
) -> ValidResourceBounds {
    create_all_resource_bounds(
        l1_gas,
        DEFAULT_STRK_L1_GAS_PRICE.into(),
        l2_gas,
        DEFAULT_STRK_L2_GAS_PRICE.into(),
        l1_data_gas,
        DEFAULT_STRK_L1_DATA_GAS_PRICE.into(),
    )
}

/// Struct containing the data usually needed to initialize a test.
pub struct TestInitData {
    pub state: CachedState<DictStateReader>,
    pub account_address: ContractAddress,
    pub contract_address: ContractAddress,
    pub nonce_manager: NonceManager,
}

/// Deploys a new account with the given class hash, funds with both fee tokens, and returns the
/// deploy tx and address.
pub fn deploy_and_fund_account(
    state: &mut CachedState<DictStateReader>,
    nonce_manager: &mut NonceManager,
    chain_info: &ChainInfo,
    deploy_tx_args: DeployAccountTxArgs,
) -> (AccountTransaction, ContractAddress) {
    // Deploy an account contract.
    let deploy_account_tx = AccountTransaction::new_with_default_flags(
        create_executable_deploy_account_tx_and_update_nonce(deploy_tx_args, nonce_manager),
    );
    let account_address = deploy_account_tx.sender_address();

    // Update the balance of the about-to-be deployed account contract in the erc20 contract, so it
    // can pay for the transaction execution.
    // Set balance in all fee types.
    let deployed_account_balance_key = get_fee_token_var_address(account_address);
    for fee_type in FeeType::iter() {
        let fee_token_address = chain_info.fee_token_address(&fee_type);
        state
            .set_storage_at(fee_token_address, deployed_account_balance_key, felt!(BALANCE.0))
            .unwrap();
    }

    (deploy_account_tx, account_address)
}

/// Initializes a state and returns a `TestInitData` instance.
pub fn create_test_init_data(chain_info: &ChainInfo, cairo_version: CairoVersion) -> TestInitData {
    let account = FeatureContract::AccountWithoutValidations(cairo_version);
    let test_contract = FeatureContract::TestContract(cairo_version);
    let erc20 = FeatureContract::ERC20(CairoVersion::Cairo0);
    let state = test_state(chain_info, BALANCE, &[(account, 1), (erc20, 1), (test_contract, 1)]);
    TestInitData {
        state,
        account_address: account.get_instance_address(0),
        contract_address: test_contract.get_instance_address(0),
        nonce_manager: NonceManager::default(),
    }
}

pub struct FaultyAccountTxCreatorArgs {
    pub tx_type: TransactionType,
    pub tx_version: TransactionVersion,
    pub scenario: u64,
    pub max_fee: Fee,
    pub resource_bounds: ValidResourceBounds,
    // Should be None unless scenario is CALL_CONTRACT.
    pub additional_data: Option<Vec<Felt>>,
    // Should be use with tx_type Declare or InvokeFunction.
    pub sender_address: ContractAddress,
    // Should be used with tx_type DeployAccount.
    pub class_hash: ClassHash,
    // Should be used with tx_type DeployAccount.
    pub contract_address_salt: ContractAddressSalt,
    // Should be used with tx_type DeployAccount.
    pub validate_constructor: bool,
    // Should be used with tx_type Declare.
    pub declared_contract: Option<FeatureContract>,
    // Execution flags.
    pub validate: bool,
    pub only_query: bool,
    pub charge_fee: bool,
}

impl Default for FaultyAccountTxCreatorArgs {
    fn default() -> Self {
        Self {
            tx_type: TransactionType::InvokeFunction,
            tx_version: TransactionVersion::THREE,
            scenario: VALID,
            additional_data: None,
            sender_address: ContractAddress::default(),
            class_hash: ClassHash::default(),
            contract_address_salt: ContractAddressSalt::default(),
            validate_constructor: false,
            max_fee: Fee::default(),
            resource_bounds: ValidResourceBounds::create_for_testing_no_fee_enforcement(),
            declared_contract: None,
            validate: true,
            only_query: false,
            charge_fee: true,
        }
    }
}

/// This function is similar to the function 'create_account_tx_for_validate_test' except it ignores
/// the nonce manager. Should be used for transactions which are expected to fail or if the nonce is
/// irrelevant to the test.
pub fn create_account_tx_for_validate_test_nonce_0(
    faulty_account_tx_creator_args: FaultyAccountTxCreatorArgs,
) -> AccountTransaction {
    create_account_tx_for_validate_test(
        &mut NonceManager::default(),
        faulty_account_tx_creator_args,
    )
}

/// Creates an account transaction to test the 'validate' method of account transactions. The
/// transaction is formatted to work with the account contract 'FaultyAccount'. These transactions
/// should be used for unit tests. For example, it is not intended to deploy a contract
/// and later call it.
pub fn create_account_tx_for_validate_test(
    nonce_manager: &mut NonceManager,
    faulty_account_tx_creator_args: FaultyAccountTxCreatorArgs,
) -> AccountTransaction {
    // TODO(Yoni): add `strict_nonce_check` to this struct.
    let FaultyAccountTxCreatorArgs {
        tx_type,
        tx_version,
        scenario,
        additional_data,
        sender_address,
        class_hash,
        contract_address_salt,
        validate_constructor,
        max_fee,
        resource_bounds,
        declared_contract,
        validate,
        only_query,
        charge_fee,
    } = faulty_account_tx_creator_args;

    // The first felt of the signature is used to set the scenario. If the scenario is
    // `CALL_CONTRACT` the second felt is used to pass the contract address.
    let mut signature_vector = vec![Felt::from(scenario)];
    if let Some(additional_data) = additional_data {
        signature_vector.extend(additional_data);
    }
    let signature = TransactionSignature(signature_vector.into());
    let execution_flags =
        ExecutionFlags { validate, charge_fee, only_query, strict_nonce_check: true };
    match tx_type {
        TransactionType::Declare => {
            let declared_contract = match declared_contract {
                Some(declared_contract) => declared_contract,
                None => {
                    // It does not matter which class is declared for this test.
                    FeatureContract::TestContract(CairoVersion::from_declare_tx_version(tx_version))
                }
            };
            let class_hash = declared_contract.get_class_hash();
            let class_info = calculate_class_info_for_testing(declared_contract.get_class());
            let tx = executable_declare_tx(
                declare_tx_args! {
                    max_fee,
                    resource_bounds,
                    signature,
                    sender_address,
                    version: tx_version,
                    nonce: nonce_manager.next(sender_address),
                    class_hash,
                    compiled_class_hash: declared_contract.get_compiled_class_hash(),
                },
                class_info,
            );
            AccountTransaction { tx, execution_flags }
        }
        TransactionType::DeployAccount => {
            // We do not use the sender address here because the transaction generates the actual
            // sender address.
            let constructor_calldata = calldata![felt!(match validate_constructor {
                true => constants::FELT_TRUE,
                false => constants::FELT_FALSE,
            })];
            let tx = create_executable_deploy_account_tx_and_update_nonce(
                deploy_account_tx_args! {
                    max_fee,
                    resource_bounds,
                    signature,
                    version: tx_version,
                    class_hash,
                    contract_address_salt,
                    constructor_calldata,
                },
                nonce_manager,
            );
            AccountTransaction { tx, execution_flags }
        }
        TransactionType::InvokeFunction => {
            let execute_calldata = create_calldata(sender_address, "foo", &[]);
            let tx = executable_invoke_tx(invoke_tx_args! {
                max_fee,
                resource_bounds,
                signature,
                sender_address,
                calldata: execute_calldata,
                version: tx_version,
                nonce: nonce_manager.next(sender_address),

            });
            AccountTransaction { tx, execution_flags }
        }
        _ => panic!("{tx_type:?} is not an account transaction."),
    }
}

pub fn invoke_tx_with_default_flags(invoke_args: InvokeTxArgs) -> AccountTransaction {
    let tx = executable_invoke_tx(invoke_args);
    AccountTransaction::new_with_default_flags(tx)
}

pub fn run_invoke_tx(
    state: &mut CachedState<DictStateReader>,
    block_context: &BlockContext,
    invoke_args: InvokeTxArgs,
) -> TransactionExecutionResult<TransactionExecutionInfo> {
    let tx = executable_invoke_tx(invoke_args);
    let account_tx = AccountTransaction::new_for_sequencing(tx);

    account_tx.execute(state, block_context)
}

/// Creates a `ResourceBoundsMapping` with the given `max_amount` and `max_price` for L1 gas limits.
/// No guarantees on the values of the other resources bounds.
// TODO(Dori): Check usages of this function and update to using all gas bounds.
pub fn l1_resource_bounds(
    max_amount: GasAmount,
    max_price_per_unit: GasPrice,
) -> ValidResourceBounds {
    ValidResourceBounds::L1Gas(ResourceBounds { max_amount, max_price_per_unit })
}

#[fixture]
pub fn all_resource_bounds(
    #[default(DEFAULT_L1_GAS_AMOUNT)] l1_max_amount: GasAmount,
    #[default(GasPrice::from(DEFAULT_STRK_L1_GAS_PRICE))] l1_max_price: GasPrice,
    #[default(DEFAULT_L2_GAS_MAX_AMOUNT)] l2_max_amount: GasAmount,
    #[default(GasPrice::from(DEFAULT_STRK_L2_GAS_PRICE))] l2_max_price: GasPrice,
    #[default(DEFAULT_L1_DATA_GAS_MAX_AMOUNT)] l1_data_max_amount: GasAmount,
    #[default(GasPrice::from(DEFAULT_STRK_L1_DATA_GAS_PRICE))] l1_data_max_price: GasPrice,
) -> ValidResourceBounds {
    create_all_resource_bounds(
        l1_max_amount,
        l1_max_price,
        l2_max_amount,
        l2_max_price,
        l1_data_max_amount,
        l1_data_max_price,
    )
}

pub fn create_all_resource_bounds(
    l1_max_amount: GasAmount,
    l1_max_price: GasPrice,
    l2_max_amount: GasAmount,
    l2_max_price: GasPrice,
    l1_data_max_amount: GasAmount,
    l1_data_max_price: GasPrice,
) -> ValidResourceBounds {
    ValidResourceBounds::AllResources(AllResourceBounds {
        l1_gas: ResourceBounds { max_amount: l1_max_amount, max_price_per_unit: l1_max_price },
        l2_gas: ResourceBounds { max_amount: l2_max_amount, max_price_per_unit: l2_max_price },
        l1_data_gas: ResourceBounds {
            max_amount: l1_data_max_amount,
            max_price_per_unit: l1_data_max_price,
        },
    })
}

pub fn calculate_class_info_for_testing(contract_class: ContractClass) -> ClassInfo {
    let (sierra_program_length, sierra_version) = match contract_class {
        ContractClass::V0(_) => (0, SierraVersion::DEPRECATED),
        ContractClass::V1(_) => (100, SierraVersion::LATEST),
    };
    ClassInfo::new(&contract_class, sierra_program_length, 100, sierra_version).unwrap()
}

pub fn emit_n_events_tx(
    n: usize,
    account_contract: ContractAddress,
    contract_address: ContractAddress,
    nonce: Nonce,
) -> AccountTransaction {
    let entry_point_args = vec![
        felt!(u32::try_from(n).unwrap()), // events_number.
        felt!(0_u32),                     // keys length.
        felt!(0_u32),                     // data length.
    ];
    let calldata = create_calldata(contract_address, "test_emit_events", &entry_point_args);
    let tx = executable_invoke_tx(invoke_tx_args! {
        sender_address: account_contract,
        calldata,
        nonce,
    });

    AccountTransaction::new_for_sequencing(tx)
}
