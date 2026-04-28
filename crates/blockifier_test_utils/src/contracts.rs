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
use starknet_api::{class_hash, contract_address};
use strum::{EnumIter, IntoEnumIterator};

use crate::cairo_compile::{
    allowed_libfuncs_json_path,
    allowed_libfuncs_legacy_json_path,
    cairo0_compile,
    cairo1_compile,
    CompilationArtifacts,
    LibfuncArg,
};
use crate::cairo_versions::{CairoVersion, RunnableCairo1};
use crate::compile_cache;

#[cfg(test)]
#[path = "contracts_test.rs"]
mod contracts_test;

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
const DELEGATE_PROXY_BASE: u32 = 13 * CLASS_HASH_BASE;
const TEST_CONTRACT2_BASE: u32 = 14 * CLASS_HASH_BASE;
const EXPERIMENTAL_CONTRACT_BASE: u32 = 15 * CLASS_HASH_BASE;
const TX_INFO_WRITER_CONTRACT_BASE: u32 = 16 * CLASS_HASH_BASE;
const BLOCK_INFO_TEST_CONTRACT_BASE: u32 = 17 * CLASS_HASH_BASE;
const MOCK_STAKING_CONTRACT_BASE: u32 = 18 * CLASS_HASH_BASE;
const FUZZ_TEST_BASE: u32 = 19 * CLASS_HASH_BASE;
const FUZZ_TEST2_BASE: u32 = 20 * CLASS_HASH_BASE;
const FUZZ_TEST_ORCHESTRATOR_BASE: u32 = 21 * CLASS_HASH_BASE;
const ACCOUNT_WITH_REAL_VALIDATE_BASE: u32 = 22 * CLASS_HASH_BASE;

// Contract names.
const ACCOUNT_LONG_VALIDATE_NAME: &str = "account_with_long_validate";
const ACCOUNT_WITHOUT_VALIDATIONS_NAME: &str = "account_with_dummy_validate";
const BLOCK_INFO_TEST_CONTRACT_NAME: &str = "block_info_test_contract";
const DELEGATE_PROXY_NAME: &str = "delegate_proxy";
const EMPTY_CONTRACT_NAME: &str = "empty_contract";
const FAULTY_ACCOUNT_NAME: &str = "account_faulty";
const LEGACY_CONTRACT_NAME: &str = "legacy_test_contract";
const SECURITY_TEST_CONTRACT_NAME: &str = "security_tests_contract";
const TEST_CONTRACT_NAME: &str = "test_contract";
const TEST_CONTRACT2_NAME: &str = "test_contract2";
const CAIRO_STEPS_TEST_CONTRACT_NAME: &str = "cairo_steps_test_contract";
const EXECUTION_INFO_V1_CONTRACT_NAME: &str = "test_contract_execution_info_v1";
const EMPTY_ACCOUNT_NAME: &str = "empty_account";
const META_TX_CONTRACT_NAME: &str = "meta_tx_test_contract";
const MOCK_STAKING_CONTRACT_NAME: &str = "mock_staking";
const EXPERIMENTAL_CONTRACT_NAME: &str = "experimental_contract";
const TX_INFO_WRITER_CONTRACT_NAME: &str = "tx_info_writer";
const FUZZ_TEST_NAME: &str = "fuzz_revert";
const FUZZ_TEST2_NAME: &str = "fuzz_revert_2";
const FUZZ_TEST_ORCHESTRATOR_NAME: &str = "fuzz_revert_orchestrator";
const ACCOUNT_WITH_REAL_VALIDATE_NAME: &str = "account_with_real_validate";
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
    BlockInfoTestContract(CairoVersion),
    DelegateProxy,
    EmptyAccount(RunnableCairo1),
    ERC20(CairoVersion),
    Empty(CairoVersion),
    Experimental,
    FaultyAccount(CairoVersion),
    LegacyTestContract,
    SecurityTests,
    TestContract(CairoVersion),
    TestContract2,
    CairoStepsTestContract,
    SierraExecutionInfoV1Contract(RunnableCairo1),
    MetaTx(RunnableCairo1),
    MockStakingContract(RunnableCairo1),
    TxInfoWriter,
    FuzzTest(CairoVersion),
    FuzzTest2(RunnableCairo1),
    FuzzTestOrchestrator(RunnableCairo1),
    AccountWithRealValidate(RunnableCairo1),
}

impl FeatureContract {
    pub fn cairo_version(&self) -> CairoVersion {
        match self {
            Self::AccountWithLongValidate(version)
            | Self::AccountWithoutValidations(version)
            | Self::BlockInfoTestContract(version)
            | Self::Empty(version)
            | Self::FaultyAccount(version)
            | Self::TestContract(version)
            | Self::ERC20(version)
            | Self::FuzzTest(version) => *version,
            Self::DelegateProxy
            | Self::SecurityTests
            | Self::TestContract2
            | Self::TxInfoWriter => CairoVersion::Cairo0,
            Self::LegacyTestContract | Self::CairoStepsTestContract | Self::Experimental => {
                CairoVersion::Cairo1(RunnableCairo1::Casm)
            }
            Self::SierraExecutionInfoV1Contract(runnable_version)
            | Self::MetaTx(runnable_version)
            | Self::EmptyAccount(runnable_version)
            | Self::MockStakingContract(runnable_version)
            | Self::FuzzTest2(runnable_version)
            | Self::FuzzTestOrchestrator(runnable_version)
            | Self::AccountWithRealValidate(runnable_version) => {
                CairoVersion::Cairo1(*runnable_version)
            }
        }
    }

    pub fn set_cairo_version(&mut self, version: CairoVersion) {
        match self {
            Self::AccountWithLongValidate(v)
            | Self::AccountWithoutValidations(v)
            | Self::BlockInfoTestContract(v)
            | Self::Empty(v)
            | Self::FaultyAccount(v)
            | Self::TestContract(v)
            | Self::ERC20(v)
            | Self::FuzzTest(v) => *v = version,
            Self::SierraExecutionInfoV1Contract(rv)
            | Self::MetaTx(rv)
            | Self::EmptyAccount(rv)
            | Self::MockStakingContract(rv)
            | Self::FuzzTest2(rv)
            | Self::FuzzTestOrchestrator(rv)
            | Self::AccountWithRealValidate(rv) => match version {
                CairoVersion::Cairo0 => panic!("{self:?} must be Cairo1"),
                CairoVersion::Cairo1(runnable) => *rv = runnable,
            },
            Self::DelegateProxy
            | Self::SecurityTests
            | Self::TestContract2
            | Self::CairoStepsTestContract
            | Self::Experimental
            | Self::LegacyTestContract
            | Self::TxInfoWriter => {
                panic!("{self:?} contract has no configurable version.")
            }
        }
    }

    pub fn get_class_hash(&self) -> ClassHash {
        class_hash!(self.get_integer_base())
    }

    /// Returns the compiled class hash for the given hash version.
    /// For Cairo 1 contracts the hash is computed from the CASM artifact and cached on disk;
    /// for Cairo 0 contracts it is always the default (zero) hash.
    pub fn get_compiled_class_hash(&self, hash_version: &HashVersion) -> CompiledClassHash {
        match self.cairo_version() {
            CairoVersion::Cairo0 => CompiledClassHash::default(),
            CairoVersion::Cairo1(_) => {
                self.ensure_compiled_class_hashes_cached();
                compile_cache::read_cached_compiled_class_hash(self, hash_version)
            }
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
        self.get_sierra().get_sierra_version().unwrap()
    }

    pub fn get_raw_class(&self) -> String {
        get_raw_contract_class(&self.get_compiled_path())
    }

    /// Ensures that compiled artifacts are available for this contract.
    /// For Cairo 1 non-ERC20 contracts, this triggers on-demand compilation with caching.
    /// For Cairo 0 and ERC20 contracts (which use committed artifacts), this is a no-op.
    fn ensure_compiled(&self) {
        if matches!(self, Self::ERC20(_)) || self.cairo_version().is_cairo0() {
            return;
        }
        compile_cache::ensure_cairo1_compiled(self);
    }

    /// Ensures that compiled class hash files are cached for this contract.
    /// For non-ERC20 Cairo 1 contracts this is part of `ensure_compiled`.
    /// For ERC20 this computes hashes from the committed CASM artifact.
    fn ensure_compiled_class_hashes_cached(&self) {
        if matches!(self, Self::ERC20(CairoVersion::Cairo1(_))) {
            compile_cache::ensure_erc20_compiled_class_hashes();
        } else {
            self.ensure_compiled();
        }
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

    /// Unique integer representing each unique contract. Used to derive "class hash", "address" and
    /// "compiled class hash".
    pub fn get_integer_base(self) -> u32 {
        self.get_cairo_version_bit()
            + match self {
                Self::AccountWithLongValidate(_) => ACCOUNT_LONG_VALIDATE_BASE,
                Self::AccountWithoutValidations(_) => ACCOUNT_WITHOUT_VALIDATIONS_BASE,
                Self::BlockInfoTestContract(_) => BLOCK_INFO_TEST_CONTRACT_BASE,
                Self::DelegateProxy => DELEGATE_PROXY_BASE,
                Self::TestContract2 => TEST_CONTRACT2_BASE,
                Self::Empty(_) => EMPTY_CONTRACT_BASE,
                Self::ERC20(_) => ERC20_CONTRACT_BASE,
                Self::Experimental => EXPERIMENTAL_CONTRACT_BASE,
                Self::FaultyAccount(_) => FAULTY_ACCOUNT_BASE,
                Self::LegacyTestContract => LEGACY_CONTRACT_BASE,
                Self::SecurityTests => SECURITY_TEST_CONTRACT_BASE,
                Self::TestContract(_) => TEST_CONTRACT_BASE,
                Self::CairoStepsTestContract => CAIRO_STEPS_TEST_CONTRACT_BASE,
                Self::SierraExecutionInfoV1Contract(_) => SIERRA_EXECUTION_INFO_V1_CONTRACT_BASE,
                Self::EmptyAccount(_) => EMPTY_ACCOUNT_BASE,
                Self::MetaTx(_) => META_TX_CONTRACT_BASE,
                Self::MockStakingContract(_) => MOCK_STAKING_CONTRACT_BASE,
                Self::TxInfoWriter => TX_INFO_WRITER_CONTRACT_BASE,
                Self::FuzzTest(_) => FUZZ_TEST_BASE,
                Self::FuzzTest2(_) => FUZZ_TEST2_BASE,
                Self::FuzzTestOrchestrator(_) => FUZZ_TEST_ORCHESTRATOR_BASE,
                Self::AccountWithRealValidate(_) => ACCOUNT_WITH_REAL_VALIDATE_BASE,
            }
    }

    /// Returns a base name for this contract, usable for cache file naming.
    pub fn get_base_name(&self) -> &str {
        match self {
            Self::ERC20(_) => "erc20",
            other => other.get_non_erc20_base_name(),
        }
    }

    pub fn get_non_erc20_base_name(&self) -> &str {
        match self {
            Self::AccountWithLongValidate(_) => ACCOUNT_LONG_VALIDATE_NAME,
            Self::AccountWithoutValidations(_) => ACCOUNT_WITHOUT_VALIDATIONS_NAME,
            Self::BlockInfoTestContract(_) => BLOCK_INFO_TEST_CONTRACT_NAME,
            Self::DelegateProxy => DELEGATE_PROXY_NAME,
            Self::Experimental => EXPERIMENTAL_CONTRACT_NAME,
            Self::TestContract2 => TEST_CONTRACT2_NAME,
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
            Self::TxInfoWriter => TX_INFO_WRITER_CONTRACT_NAME,
            Self::FuzzTest(_) => FUZZ_TEST_NAME,
            Self::FuzzTest2(_) => FUZZ_TEST2_NAME,
            Self::FuzzTestOrchestrator(_) => FUZZ_TEST_ORCHESTRATOR_NAME,
            Self::AccountWithRealValidate(_) => ACCOUNT_WITH_REAL_VALIDATE_NAME,
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
        self.ensure_compiled();
        if matches!(self, &Self::ERC20(CairoVersion::Cairo1(_))) {
            return ERC20_SIERRA_CONTRACT_PATH.to_string();
        }

        compile_cache::cached_sierra_path(self).to_string_lossy().to_string()
    }

    pub fn get_compiled_path(&self) -> String {
        self.ensure_compiled();
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
            match self.cairo_version() {
                CairoVersion::Cairo0 => {
                    format!(
                        "resources/feature_contracts/cairo0/compiled/{}_compiled.json",
                        self.get_non_erc20_base_name()
                    )
                }
                CairoVersion::Cairo1(RunnableCairo1::Casm) => {
                    compile_cache::cached_compiled_path(self).to_string_lossy().to_string()
                }
                #[cfg(feature = "cairo_native")]
                CairoVersion::Cairo1(RunnableCairo1::Native) => {
                    compile_cache::cached_sierra_path(self).to_string_lossy().to_string()
                }
            }
        }
    }

    /// Returns the libfunc list argument for compiling this Cairo 1 contract.
    pub fn libfunc_arg(&self) -> LibfuncArg {
        match self {
            Self::Experimental => {
                LibfuncArg::ListFile("./resources/experimental_libfuncs.json".to_string())
            }
            Self::LegacyTestContract | Self::CairoStepsTestContract => {
                LibfuncArg::ListFile(allowed_libfuncs_legacy_json_path())
            }
            _ => LibfuncArg::ListFile(allowed_libfuncs_json_path()),
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
                    | FeatureContract::BlockInfoTestContract(_)
                    | FeatureContract::FaultyAccount(_)
                    | FeatureContract::TxInfoWriter => Some("--account_contract".into()),
                    FeatureContract::SecurityTests => Some("--disable_hint_validation".into()),
                    FeatureContract::DelegateProxy
                    | FeatureContract::Empty(_)
                    | FeatureContract::TestContract(_)
                    | FeatureContract::TestContract2
                    | FeatureContract::LegacyTestContract
                    | FeatureContract::CairoStepsTestContract
                    | FeatureContract::SierraExecutionInfoV1Contract(_)
                    | FeatureContract::EmptyAccount(_)
                    | FeatureContract::MetaTx(_)
                    | FeatureContract::MockStakingContract(_)
                    | FeatureContract::FuzzTest(_)
                    | FeatureContract::FuzzTest2(_)
                    | FeatureContract::FuzzTestOrchestrator(_)
                    | FeatureContract::AccountWithRealValidate(_) => None,
                    FeatureContract::ERC20(_) | FeatureContract::Experimental => unreachable!(),
                };
                cairo0_compile(self.get_source_path(), extra_arg, false)
            }
            CairoVersion::Cairo1(_) => {
                cairo1_compile(self.get_source_path(), self.fixed_version(), self.libfunc_arg())
            }
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
            | Self::BlockInfoTestContract(_)
            | Self::Empty(_)
            | Self::FaultyAccount(_)
            | Self::TestContract(_)
            | Self::ERC20(_)
            | Self::FuzzTest(_) => {
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
            | Self::MockStakingContract(_)
            | Self::FuzzTest2(_)
            | Self::FuzzTestOrchestrator(_)
            | Self::AccountWithRealValidate(_) => {
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

            Self::DelegateProxy
            | Self::LegacyTestContract
            | Self::CairoStepsTestContract
            | Self::Experimental
            | Self::TestContract2
            | Self::SecurityTests
            | Self::TxInfoWriter => {
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

/// Reads the raw JSON content of a contract class from disk.
/// Accepts both absolute paths (e.g. from the compilation cache) and crate-relative paths
/// (e.g. `resources/feature_contracts/...` resolved against the crate's manifest directory).
pub fn get_raw_contract_class(contract_path: &str) -> String {
    let path: PathBuf = if std::path::Path::new(contract_path).is_absolute() {
        PathBuf::from(contract_path)
    } else {
        [compile_time_cargo_manifest_dir!(), contract_path].iter().collect()
    };
    fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read contract from {path:?}: {e}"))
}
