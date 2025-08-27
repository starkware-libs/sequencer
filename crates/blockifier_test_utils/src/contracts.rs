use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use apollo_infra_utils::cairo_compiler_version::CAIRO1_COMPILER_VERSION;
use apollo_infra_utils::compile_time_cargo_manifest_dir;
use cairo_lang_starknet_classes::contract_class::ContractClass as CairoLangContractClass;
use starknet_api::contract_class::compiled_class_hash::HashVersion;
use starknet_api::contract_class::SierraVersion;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress};
use starknet_api::state::SierraContractClass;
use starknet_api::{class_hash, contract_address, felt};
use starknet_types_core::felt::Felt;
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

use crate::cairo_compile::{cairo0_compile, cairo1_compile, CompilationArtifacts};
use crate::cairo_versions::{CairoVersion, RunnableCairo1};

pub const CAIRO1_FEATURE_CONTRACTS_DIR: &str = "resources/feature_contracts/cairo1";
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
const META_TX_CONTRACT_BASE: u32 = 11 * CLASS_HASH_BASE;
const EMPTY_ACCOUNT_BASE: u32 = 12 * CLASS_HASH_BASE;
const MOCK_STAKING_CONTRACT_BASE: u32 = 12 * CLASS_HASH_BASE;

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
const EMPTY_ACCOUNT_NAME: &str = "empty_account";
const META_TX_CONTRACT_NAME: &str = "meta_tx_test_contract";
const MOCK_STAKING_CONTRACT_NAME: &str = "mock_staking";
// ERC20 contract is in a unique location.
const ERC20_CAIRO0_CONTRACT_SOURCE_PATH: &str =
    "./resources/ERC20/ERC20_Cairo0/ERC20_without_some_syscalls/ERC20/ERC20.cairo";
const ERC20_CAIRO0_CONTRACT_PATH: &str = "./resources/ERC20/ERC20_Cairo0/\
                                          ERC20_without_some_syscalls/ERC20/\
                                          erc20_contract_without_some_syscalls_compiled.json";
const ERC20_CAIRO1_CONTRACT_SOURCE_PATH: &str = "./resources/ERC20/ERC20_Cairo1/ERC20.cairo";
const ERC20_SIERRA_CONTRACT_PATH: &str = "./resources/ERC20/ERC20_Cairo1/erc20.sierra.json";
const ERC20_CAIRO1_CONTRACT_PATH: &str = "./resources/ERC20/ERC20_Cairo1/erc20.casm.json";

// The following contracts are compiled with a fixed version of the compiler.
const LEGACY_CONTRACT_COMPILER_VERSION: &str = "2.1.0";
const CAIRO_STEPS_TEST_CONTRACT_COMPILER_VERSION: &str = "2.7.0";

pub type CairoVersionString = String;

/// Enum representing all feature contracts.
/// The contracts that are implemented in both Cairo versions include a version field.
#[derive(Clone, Copy, Debug, EnumIter, Hash, PartialEq, Eq)]
pub enum FeatureContract {
    AccountWithLongValidate(CairoVersion),
    AccountWithoutValidations(CairoVersion),
    EmptyAccount(RunnableCairo1),
    ERC20(CairoVersion),
    Empty(CairoVersion),
    FaultyAccount(CairoVersion),
    LegacyTestContract,
    SecurityTests,
    TestContract(CairoVersion),
    CairoStepsTestContract,
    SierraExecutionInfoV1Contract(RunnableCairo1),
    MetaTx(RunnableCairo1),
    MockStakingContract(RunnableCairo1),
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
            Self::SierraExecutionInfoV1Contract(runnable_version)
            | Self::MetaTx(runnable_version)
            | Self::EmptyAccount(runnable_version)
            | Self::MockStakingContract(runnable_version) => {
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
            Self::SierraExecutionInfoV1Contract(rv)
            | Self::MetaTx(rv)
            | Self::EmptyAccount(rv)
            | Self::MockStakingContract(rv) => match version {
                CairoVersion::Cairo0 => panic!("{self:?} must be Cairo1"),
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

    // Returns dummy compiled class hash for the given hash version.
    pub fn get_compiled_class_hash(&self, hash_version: &HashVersion) -> CompiledClassHash {
        let base_felt = felt!(self.get_integer_base());

        let hash_value = match (self.cairo_version(), hash_version) {
            (CairoVersion::Cairo0, _) => Felt::ZERO,
            (CairoVersion::Cairo1(_), HashVersion::V1) => base_felt,
            (CairoVersion::Cairo1(_), _) => base_felt + 100,
        };
        CompiledClassHash(hash_value)
    }
    /// Returns the address of the instance with the given instance ID.
    pub fn instance_address(integer_base: u32, instance_id: u32) -> ContractAddress {
        contract_address!(integer_base + instance_id + ADDRESS_BIT)
    }

    /// Returns the address of the instance with the given instance ID.
    pub fn get_instance_address(&self, instance_id: u16) -> ContractAddress {
        Self::instance_address(self.get_integer_base(), instance_id.into())
    }

    pub fn get_raw_sierra(&self) -> Option<String> {
        if self.cairo_version() == CairoVersion::Cairo0 {
            return None;
        }

        Some(get_raw_contract_class(&self.get_sierra_path()))
    }

    pub fn get_sierra(&self) -> SierraContractClass {
        self.safe_get_sierra().expect("The sierra contract is only available for Cairo1.")
    }

    // TODO(AvivG): Consider unify get_sierra and get_runnable.
    pub fn safe_get_sierra(&self) -> Option<SierraContractClass> {
        let raw_sierra = self.get_raw_sierra()?;
        let cairo_contract_class: CairoLangContractClass =
            serde_json::from_str(&raw_sierra).unwrap();
        Some(SierraContractClass::from(cairo_contract_class))
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

    /// Some Cairo1 contracts are designed to test behavior of code compiled with a specific (old)
    /// compiler version. Returns the compiler version used to compile the contract.
    /// Panics if called on a Cairo0 contract.
    pub fn fixed_version(&self) -> CairoVersionString {
        match self {
            Self::LegacyTestContract => LEGACY_CONTRACT_COMPILER_VERSION.into(),
            Self::CairoStepsTestContract => CAIRO_STEPS_TEST_CONTRACT_COMPILER_VERSION.into(),
            contract => {
                assert!(
                    !contract.cairo_version().is_cairo0(),
                    "fixed_version() should only be called for Cairo1 contracts."
                );
                CAIRO1_COMPILER_VERSION.to_string()
            }
        }
    }

    /// Unique integer representing each unique contract. Used to derive "class hash" and "address".
    pub fn get_integer_base(self) -> u32 {
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
                Self::EmptyAccount(_) => EMPTY_ACCOUNT_BASE,
                Self::MetaTx(_) => META_TX_CONTRACT_BASE,
                Self::MockStakingContract(_) => MOCK_STAKING_CONTRACT_BASE,
            }
    }

    pub fn get_non_erc20_base_name(&self) -> &str {
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
            Self::EmptyAccount(_) => EMPTY_ACCOUNT_NAME,
            Self::MetaTx(_) => META_TX_CONTRACT_NAME,
            Self::MockStakingContract(_) => MOCK_STAKING_CONTRACT_NAME,
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
                "resources/feature_contracts/cairo{}/{}.cairo",
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
                CairoVersion::Cairo1(RunnableCairo1::Native) => ERC20_SIERRA_CONTRACT_PATH,
            }
            .into()
        } else {
            let cairo_version = self.cairo_version();
            format!(
                "resources/feature_contracts/cairo{}/{}{}.json",
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
                    | FeatureContract::SierraExecutionInfoV1Contract(_)
                    | FeatureContract::EmptyAccount(_)
                    | FeatureContract::MetaTx(_)
                    | FeatureContract::MockStakingContract(_) => None,
                    FeatureContract::ERC20(_) => unreachable!(),
                };
                cairo0_compile(self.get_source_path(), extra_arg, false)
            }
            CairoVersion::Cairo1(_) => cairo1_compile(self.get_source_path(), self.fixed_version()),
        }
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

            Self::SierraExecutionInfoV1Contract(_)
            | Self::MetaTx(_)
            | Self::EmptyAccount(_)
            | Self::MockStakingContract(_) => {
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

    pub fn all_cairo1_casm_feature_contracts() -> impl Iterator<Item = Self> {
        Self::all_feature_contracts().filter(|contract| {
            matches!(contract.cairo_version(), CairoVersion::Cairo1(RunnableCairo1::Casm))
        })
    }

    pub fn all_cairo1_casm_compiler_versions() -> HashSet<CairoVersionString> {
        Self::all_feature_contracts()
            .filter(|contract| {
                contract.cairo_version() == CairoVersion::Cairo1(RunnableCairo1::Casm)
            })
            .map(|contract| contract.fixed_version())
            .collect()
    }
}

pub fn get_raw_contract_class(contract_path: &str) -> String {
    let path: PathBuf = [compile_time_cargo_manifest_dir!(), contract_path].iter().collect();
    fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read contract from {path:?}: {e}"))
}
