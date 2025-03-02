use std::collections::HashMap;

use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_starknet_classes::contract_class::ContractClass as CairoLangContractClass;
use itertools::Itertools;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::abi::constants::CONSTRUCTOR_ENTRY_POINT_NAME;
use starknet_api::contract_class::{ContractClass, EntryPointType, SierraVersion};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, EntryPointSelector};
use starknet_api::deprecated_contract_class::{
    ContractClass as DeprecatedContractClass,
    EntryPointOffset,
};
use starknet_api::state::SierraContractClass;
use starknet_api::{class_hash, contract_address, felt};
use starknet_types_core::felt::Felt;
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

use crate::execution::contract_class::RunnableCompiledClass;
use crate::execution::entry_point::EntryPointTypeAndSelector;
#[cfg(feature = "cairo_native")]
use crate::execution::native::contract_class::NativeCompiledClassV1;
use crate::test_utils::cairo_compile::{cairo0_compile, cairo1_compile, CompilationArtifacts};
use crate::test_utils::struct_impls::LoadContractFromFile;
use crate::test_utils::{get_raw_contract_class, CairoVersion, RunnableCairo1};

pub const CAIRO1_FEATURE_CONTRACTS_DIR: &str = "feature_contracts/cairo1";
pub const SIERRA_CONTRACTS_SUBDIR: &str = "sierra";

// This file contains featured contracts, used for tests. Use the function 'test_state' in
// initial_test_state.rs to initialize a state with these contracts.
//
// Use the mock class hashes and addresses to interact with the contracts in tests.
// The structure of such mock address / class hash is as follows:
// +-+-+-----------+---------------+---------------+---------------+
// |v|a| reserved  | 8 bits: class | 16 bits : address             |
// +-+-+-----------+---------------+---------------+---------------+
// v: 1 bit. 0 for Cairo0, 1 for Cairo1. bit 31.
// a: 1 bit. 0 for class hash, 1 for address. bit 30.
// reserved: Must be 0. bit 29-24.
// class: 8 bits. The class hash of the contract. bit 23-16. allows up to 256 unique contracts.
// address: 16 bits. The instance ID of the contract. bit 15-0. allows up to 65536 instances of each
// contract.

// Bit to set on class hashes and addresses of feature contracts to indicate the Cairo1 variant.
const CAIRO1_BIT: u32 = 1 << 31;

// Bit to set on a class hash to convert it to the respective address.
const ADDRESS_BIT: u32 = 1 << 30;

// Mock class hashes of the feature contract. Keep the bottom 16 bits of each class hash unset, to
// allow up to 65536 deployed instances of each contract.
const CLASS_HASH_BASE: u32 = 1 << 16;
const ACCOUNT_LONG_VALIDATE_BASE: u32 = CLASS_HASH_BASE;
const ACCOUNT_WITHOUT_VALIDATIONS_BASE: u32 = 2 * CLASS_HASH_BASE;
const EMPTY_CONTRACT_BASE: u32 = 3 * CLASS_HASH_BASE;
const FAULTY_ACCOUNT_BASE: u32 = 4 * CLASS_HASH_BASE;
const LEGACY_CONTRACT_BASE: u32 = 5 * CLASS_HASH_BASE;
const SECURITY_TEST_CONTRACT_BASE: u32 = 6 * CLASS_HASH_BASE;
const TEST_CONTRACT_BASE: u32 = 7 * CLASS_HASH_BASE;
const ERC20_CONTRACT_BASE: u32 = 8 * CLASS_HASH_BASE;
const CAIRO_STEPS_TEST_CONTRACT_BASE: u32 = 9 * CLASS_HASH_BASE;
const SIERRA_EXECUTION_INFO_V1_CONTRACT_BASE: u32 = 10 * CLASS_HASH_BASE;

// Contract names.
const ACCOUNT_LONG_VALIDATE_NAME: &str = "account_with_long_validate";
const ACCOUNT_WITHOUT_VALIDATIONS_NAME: &str = "account_with_dummy_validate";
const EMPTY_CONTRACT_NAME: &str = "empty_contract";
const FAULTY_ACCOUNT_NAME: &str = "account_faulty";
const LEGACY_CONTRACT_NAME: &str = "legacy_test_contract";
const SECURITY_TEST_CONTRACT_NAME: &str = "security_tests_contract";
const TEST_CONTRACT_NAME: &str = "test_contract";
const CAIRO_STEPS_TEST_CONTRACT_NAME: &str = "cairo_steps_test_contract";
const EXECUTION_INFO_V1_CONTRACT_NAME: &str = "test_contract_execution_info_v1";

// ERC20 contract is in a unique location.
const ERC20_CAIRO0_CONTRACT_SOURCE_PATH: &str =
    "./ERC20/ERC20_Cairo0/ERC20_without_some_syscalls/ERC20/ERC20.cairo";
const ERC20_CAIRO0_CONTRACT_PATH: &str = "./ERC20/ERC20_Cairo0/ERC20_without_some_syscalls/ERC20/\
                                          erc20_contract_without_some_syscalls_compiled.json";
const ERC20_CAIRO1_CONTRACT_SOURCE_PATH: &str = "./ERC20/ERC20_Cairo1/ERC20.cairo";
const ERC20_SIERRA_CONTRACT_PATH: &str = "./ERC20/ERC20_Cairo1/erc20.sierra.json";
const ERC20_CAIRO1_CONTRACT_PATH: &str = "./ERC20/ERC20_Cairo1/erc20.casm.json";

// The following contracts are compiled with a fixed version of the compiler. This compiler version
// no longer compiles with stable rust, so the toolchain is also fixed.
const LEGACY_CONTRACT_COMPILER_TAG: &str = "v2.1.0";
const LEGACY_CONTRACT_RUST_TOOLCHAIN: &str = "2023-07-05";

const CAIRO_STEPS_TEST_CONTRACT_COMPILER_TAG: &str = "v2.7.0";
const CAIRO_STEPS_TEST_CONTRACT_RUST_TOOLCHAIN: &str = "2024-04-29";

pub type TagAndToolchain = (Option<String>, Option<String>);
pub type TagToContractsMapping = HashMap<TagAndToolchain, Vec<FeatureContract>>;

/// Enum representing all feature contracts.
/// The contracts that are implemented in both Cairo versions include a version field.
#[derive(Clone, Copy, Debug, EnumIter, Hash, PartialEq, Eq)]
pub enum FeatureContract {
    AccountWithLongValidate(CairoVersion),
    AccountWithoutValidations(CairoVersion),
    ERC20(CairoVersion),
    Empty(CairoVersion),
    FaultyAccount(CairoVersion),
    LegacyTestContract,
    SecurityTests,
    TestContract(CairoVersion),
    CairoStepsTestContract,
    SierraExecutionInfoV1Contract(RunnableCairo1),
}

impl FeatureContract {
    pub fn cairo_version(&self) -> CairoVersion {
        match self {
            Self::AccountWithLongValidate(version)
            | Self::AccountWithoutValidations(version)
            | Self::Empty(version)
            | Self::FaultyAccount(version)
            | Self::TestContract(version)
            | Self::ERC20(version) => *version,
            Self::SecurityTests => CairoVersion::Cairo0,
            Self::LegacyTestContract | Self::CairoStepsTestContract => {
                CairoVersion::Cairo1(RunnableCairo1::Casm)
            }
            Self::SierraExecutionInfoV1Contract(runnable_version) => {
                CairoVersion::Cairo1(*runnable_version)
            }
        }
    }

    pub fn set_cairo_version(&mut self, version: CairoVersion) {
        match self {
            Self::AccountWithLongValidate(v)
            | Self::AccountWithoutValidations(v)
            | Self::Empty(v)
            | Self::FaultyAccount(v)
            | Self::TestContract(v)
            | Self::ERC20(v) => *v = version,
            Self::SierraExecutionInfoV1Contract(rv) => match version {
                CairoVersion::Cairo0 => panic!("SierraExecutionInfoV1Contract must be Cairo1"),
                CairoVersion::Cairo1(runnable) => *rv = runnable,
            },
            Self::SecurityTests | Self::CairoStepsTestContract | Self::LegacyTestContract => {
                panic!("{self:?} contract has no configurable version.")
            }
        }
    }

    pub fn get_class_hash(&self) -> ClassHash {
        class_hash!(self.get_integer_base())
    }

    pub fn get_compiled_class_hash(&self) -> CompiledClassHash {
        match self.cairo_version() {
            CairoVersion::Cairo0 => CompiledClassHash(Felt::ZERO),
            CairoVersion::Cairo1(_) => CompiledClassHash(felt!(self.get_integer_base())),
        }
    }

    /// Returns the address of the instance with the given instance ID.
    pub fn instance_address(integer_base: u32, instance_id: u32) -> ContractAddress {
        contract_address!(integer_base + instance_id + ADDRESS_BIT)
    }

    /// Returns the address of the instance with the given instance ID.
    pub fn get_instance_address(&self, instance_id: u16) -> ContractAddress {
        Self::instance_address(self.get_integer_base(), instance_id.into())
    }

    pub fn get_class(&self) -> ContractClass {
        match self.cairo_version() {
            CairoVersion::Cairo0 => {
                ContractClass::V0(DeprecatedContractClass::from_file(&self.get_compiled_path()))
            }
            CairoVersion::Cairo1(RunnableCairo1::Casm) => ContractClass::V1((
                CasmContractClass::from_file(&self.get_compiled_path()),
                self.get_sierra_version(),
            )),
            #[cfg(feature = "cairo_native")]
            CairoVersion::Cairo1(RunnableCairo1::Native) => {
                panic!("Native contracts are not supported by this function.")
            }
        }
    }

    pub fn get_runnable_class(&self) -> RunnableCompiledClass {
        #[cfg(feature = "cairo_native")]
        if CairoVersion::Cairo1(RunnableCairo1::Native) == self.cairo_version() {
            let native_contract_class =
                NativeCompiledClassV1::compile_or_get_cached(&self.get_compiled_path());
            return RunnableCompiledClass::V1Native(native_contract_class);
        }

        self.get_class().try_into().unwrap()
    }

    pub fn get_raw_sierra(&self) -> String {
        if self.cairo_version() == CairoVersion::Cairo0 {
            panic!("The sierra contract is only available for Cairo1.");
        }

        get_raw_contract_class(&self.get_sierra_path())
    }

    pub fn get_sierra(&self) -> SierraContractClass {
        let raw_sierra = self.get_raw_sierra();
        let cairo_contract_class: CairoLangContractClass =
            serde_json::from_str(&raw_sierra).unwrap();
        SierraContractClass::from(cairo_contract_class)
    }

    pub fn get_sierra_version(&self) -> SierraVersion {
        SierraVersion::extract_from_program(&self.get_sierra().sierra_program).unwrap()
    }

    pub fn get_raw_class(&self) -> String {
        get_raw_contract_class(&self.get_compiled_path())
    }

    fn get_cairo_version_bit(&self) -> u32 {
        match self.cairo_version() {
            CairoVersion::Cairo0 => 0,
            CairoVersion::Cairo1(_) => CAIRO1_BIT,
        }
    }

    /// Some contracts are designed to test behavior of code compiled with a
    /// specific (old) compiler tag. To run the (old) compiler, older rust
    /// version is required.
    pub fn fixed_tag_and_rust_toolchain(&self) -> TagAndToolchain {
        match self {
            Self::LegacyTestContract => (
                Some(LEGACY_CONTRACT_COMPILER_TAG.into()),
                Some(LEGACY_CONTRACT_RUST_TOOLCHAIN.into()),
            ),
            Self::CairoStepsTestContract => (
                Some(CAIRO_STEPS_TEST_CONTRACT_COMPILER_TAG.into()),
                Some(CAIRO_STEPS_TEST_CONTRACT_RUST_TOOLCHAIN.into()),
            ),
            _ => (None, None),
        }
    }

    /// Unique integer representing each unique contract. Used to derive "class hash" and "address".
    fn get_integer_base(self) -> u32 {
        self.get_cairo_version_bit()
            + match self {
                Self::AccountWithLongValidate(_) => ACCOUNT_LONG_VALIDATE_BASE,
                Self::AccountWithoutValidations(_) => ACCOUNT_WITHOUT_VALIDATIONS_BASE,
                Self::Empty(_) => EMPTY_CONTRACT_BASE,
                Self::ERC20(_) => ERC20_CONTRACT_BASE,
                Self::FaultyAccount(_) => FAULTY_ACCOUNT_BASE,
                Self::LegacyTestContract => LEGACY_CONTRACT_BASE,
                Self::SecurityTests => SECURITY_TEST_CONTRACT_BASE,
                Self::TestContract(_) => TEST_CONTRACT_BASE,
                Self::CairoStepsTestContract => CAIRO_STEPS_TEST_CONTRACT_BASE,
                Self::SierraExecutionInfoV1Contract(_) => SIERRA_EXECUTION_INFO_V1_CONTRACT_BASE,
            }
    }

    fn get_non_erc20_base_name(&self) -> &str {
        match self {
            Self::AccountWithLongValidate(_) => ACCOUNT_LONG_VALIDATE_NAME,
            Self::AccountWithoutValidations(_) => ACCOUNT_WITHOUT_VALIDATIONS_NAME,
            Self::Empty(_) => EMPTY_CONTRACT_NAME,
            Self::FaultyAccount(_) => FAULTY_ACCOUNT_NAME,
            Self::LegacyTestContract => LEGACY_CONTRACT_NAME,
            Self::SecurityTests => SECURITY_TEST_CONTRACT_NAME,
            Self::TestContract(_) => TEST_CONTRACT_NAME,
            Self::CairoStepsTestContract => CAIRO_STEPS_TEST_CONTRACT_NAME,
            Self::SierraExecutionInfoV1Contract(_) => EXECUTION_INFO_V1_CONTRACT_NAME,
            Self::ERC20(_) => unreachable!(),
        }
    }

    pub fn get_source_path(&self) -> String {
        // Special case: ERC20 contract in a different location.
        if let Self::ERC20(cairo_version) = self {
            match cairo_version {
                CairoVersion::Cairo0 => ERC20_CAIRO0_CONTRACT_SOURCE_PATH,
                CairoVersion::Cairo1(RunnableCairo1::Casm) => ERC20_CAIRO1_CONTRACT_SOURCE_PATH,
                #[cfg(feature = "cairo_native")]
                CairoVersion::Cairo1(RunnableCairo1::Native) => {
                    todo!("ERC20 contract is not supported by Native yet")
                }
            }
            .into()
        } else {
            format!(
                "feature_contracts/cairo{}/{}.cairo",
                match self.cairo_version() {
                    CairoVersion::Cairo0 => "0",
                    CairoVersion::Cairo1(_) => "1",
                },
                self.get_non_erc20_base_name()
            )
        }
    }

    pub fn get_sierra_path(&self) -> String {
        assert_ne!(self.cairo_version(), CairoVersion::Cairo0);
        // This is not the compiled Sierra file of the existing ERC20 contract,
        // but a file that was taken from the compiler repo of another ERC20 contract.
        if matches!(self, &Self::ERC20(CairoVersion::Cairo1(_))) {
            return ERC20_SIERRA_CONTRACT_PATH.to_string();
        }

        format!(
            "{CAIRO1_FEATURE_CONTRACTS_DIR}/{SIERRA_CONTRACTS_SUBDIR}/{}.sierra.json",
            self.get_non_erc20_base_name()
        )
    }

    pub fn get_compiled_path(&self) -> String {
        // ERC20 is a special case - not in the feature_contracts directory.
        if let Self::ERC20(cairo_version) = self {
            match cairo_version {
                CairoVersion::Cairo0 => ERC20_CAIRO0_CONTRACT_PATH,
                CairoVersion::Cairo1(RunnableCairo1::Casm) => ERC20_CAIRO1_CONTRACT_PATH,
                #[cfg(feature = "cairo_native")]
                CairoVersion::Cairo1(RunnableCairo1::Native) => {
                    todo!("ERC20 cannot be tested with Native")
                }
            }
            .into()
        } else {
            let cairo_version = self.cairo_version();
            format!(
                "feature_contracts/cairo{}/{}{}.json",
                match cairo_version {
                    CairoVersion::Cairo0 => "0/compiled",
                    CairoVersion::Cairo1(RunnableCairo1::Casm) => "1/compiled",
                    #[cfg(feature = "cairo_native")]
                    CairoVersion::Cairo1(RunnableCairo1::Native) => "1/sierra",
                },
                self.get_non_erc20_base_name(),
                match cairo_version {
                    CairoVersion::Cairo0 => "_compiled",
                    CairoVersion::Cairo1(RunnableCairo1::Casm) => ".casm",
                    #[cfg(feature = "cairo_native")]
                    CairoVersion::Cairo1(RunnableCairo1::Native) => ".sierra",
                }
            )
        }
    }

    /// Compiles the feature contract and returns the compiled contract as a byte vector.
    /// Panics if the contract is ERC20, as ERC20 contract recompilation is not supported.
    pub fn compile(&self) -> CompilationArtifacts {
        if matches!(self, Self::ERC20(_)) {
            panic!("ERC20 contract recompilation not supported.");
        }
        match self.cairo_version() {
            CairoVersion::Cairo0 => {
                let extra_arg: Option<String> = match self {
                    // Account contracts require the account_contract flag.
                    FeatureContract::AccountWithLongValidate(_)
                    | FeatureContract::AccountWithoutValidations(_)
                    | FeatureContract::FaultyAccount(_) => Some("--account_contract".into()),
                    FeatureContract::SecurityTests => Some("--disable_hint_validation".into()),
                    FeatureContract::Empty(_)
                    | FeatureContract::TestContract(_)
                    | FeatureContract::LegacyTestContract
                    | FeatureContract::CairoStepsTestContract
                    | FeatureContract::SierraExecutionInfoV1Contract(_) => None,
                    FeatureContract::ERC20(_) => unreachable!(),
                };
                cairo0_compile(self.get_source_path(), extra_arg, false)
            }
            CairoVersion::Cairo1(_) => {
                let (tag_override, cargo_nightly_arg) = self.fixed_tag_and_rust_toolchain();
                cairo1_compile(self.get_source_path(), tag_override, cargo_nightly_arg)
            }
        }
    }

    /// Fetch PC locations from the compiled contract to compute the expected PC locations in the
    /// traceback. Computation is not robust, but as long as the cairo function itself is not
    /// edited, this computation should be stable.
    fn get_offset(
        &self,
        entry_point_selector: EntryPointSelector,
        entry_point_type: EntryPointType,
    ) -> EntryPointOffset {
        match self.get_runnable_class() {
            RunnableCompiledClass::V0(class) => {
                class
                    .entry_points_by_type
                    .get(&entry_point_type)
                    .unwrap()
                    .iter()
                    .find(|ep| ep.selector == entry_point_selector)
                    .unwrap()
                    .offset
            }
            RunnableCompiledClass::V1(class) => {
                class
                    .entry_points_by_type
                    .get_entry_point(&EntryPointTypeAndSelector {
                        entry_point_type,
                        entry_point_selector,
                    })
                    .unwrap()
                    .offset
            }
            #[cfg(feature = "cairo_native")]
            RunnableCompiledClass::V1Native(_) => {
                panic!("Not implemented for cairo native contracts")
            }
        }
    }

    pub fn get_entry_point_offset(
        &self,
        entry_point_selector: EntryPointSelector,
    ) -> EntryPointOffset {
        self.get_offset(entry_point_selector, EntryPointType::External)
    }

    pub fn get_ctor_offset(
        &self,
        entry_point_selector: Option<EntryPointSelector>,
    ) -> EntryPointOffset {
        let selector =
            entry_point_selector.unwrap_or(selector_from_name(CONSTRUCTOR_ENTRY_POINT_NAME));
        self.get_offset(selector, EntryPointType::Constructor)
    }

    fn iter_versions(&self, versions: &[CairoVersion]) -> Vec<FeatureContract> {
        versions
            .iter()
            .map(|&v| {
                let mut versioned_contract = *self;
                versioned_contract.set_cairo_version(v);
                versioned_contract
            })
            .collect()
    }

    fn all_contract_versions(&self) -> Vec<FeatureContract> {
        match self {
            Self::AccountWithLongValidate(_)
            | Self::AccountWithoutValidations(_)
            | Self::Empty(_)
            | Self::FaultyAccount(_)
            | Self::TestContract(_)
            | Self::ERC20(_) => {
                #[cfg(not(feature = "cairo_native"))]
                let versions = [CairoVersion::Cairo0, CairoVersion::Cairo1(RunnableCairo1::Casm)];
                #[cfg(feature = "cairo_native")]
                let versions = [
                    CairoVersion::Cairo0,
                    CairoVersion::Cairo1(RunnableCairo1::Casm),
                    CairoVersion::Cairo1(RunnableCairo1::Native),
                ];
                self.iter_versions(&versions)
            }

            Self::SierraExecutionInfoV1Contract(_) => {
                #[cfg(not(feature = "cairo_native"))]
                {
                    vec![*self]
                }
                #[cfg(feature = "cairo_native")]
                {
                    let versions = [
                        CairoVersion::Cairo1(RunnableCairo1::Casm),
                        CairoVersion::Cairo1(RunnableCairo1::Native),
                    ];
                    self.iter_versions(&versions)
                }
            }

            Self::LegacyTestContract | Self::CairoStepsTestContract | Self::SecurityTests => {
                vec![*self]
            }
        }
    }

    pub fn all_contracts() -> impl Iterator<Item = Self> {
        Self::iter().flat_map(|contract| contract.all_contract_versions())
    }

    pub fn all_feature_contracts() -> impl Iterator<Item = Self> {
        // ERC20 is a special case - not in the feature_contracts directory.
        Self::all_contracts().filter(|contract| !matches!(contract, Self::ERC20(_)))
    }

    pub fn cairo1_feature_contracts_by_tag() -> TagToContractsMapping {
        Self::all_feature_contracts()
            .filter(|contract| contract.cairo_version() != CairoVersion::Cairo0)
            .map(|contract| (contract.fixed_tag_and_rust_toolchain(), contract))
            .into_group_map()
    }
}

/// The information needed to test a [FeatureContract].
pub struct FeatureContractData {
    pub class_hash: ClassHash,
    pub runnable_class: RunnableCompiledClass,
    pub require_funding: bool,
    integer_base: u32,
}
impl FeatureContractData {
    pub fn get_instance_address(&self, instance: u16) -> ContractAddress {
        // If a test requires overriding the contract address, replace storing `integer_base` in the
        // struct with storing a callback function that computes the address.
        // A test will then be able to override the callback function to return the desired address.
        FeatureContract::instance_address(self.integer_base, instance.into())
    }
}

impl From<FeatureContract> for FeatureContractData {
    fn from(contract: FeatureContract) -> Self {
        let require_funding = matches!(
            contract,
            FeatureContract::AccountWithLongValidate(_)
                | FeatureContract::AccountWithoutValidations(_)
                | FeatureContract::FaultyAccount(_)
        );

        Self {
            class_hash: contract.get_class_hash(),
            runnable_class: contract.get_runnable_class(),
            require_funding,
            integer_base: contract.get_integer_base(),
        }
    }
}
