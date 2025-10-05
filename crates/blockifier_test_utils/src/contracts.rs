use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use apollo_infra_utils::cairo_compiler_version::CAIRO1_COMPILER_VERSION;
use apollo_infra_utils::compile_time_cargo_manifest_dir;
use cairo_lang_starknet_classes::contract_class::ContractClass as CairoLangContractClass;
use expect_test::expect;
use starknet_api::contract_class::compiled_class_hash::HashVersion;
use starknet_api::contract_class::SierraVersion;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress};
use starknet_api::state::SierraContractClass;
use starknet_api::{class_hash, contract_address, felt};
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
const DELEGATE_PROXY_BASE: u32 = 13 * CLASS_HASH_BASE;
const TEST_CONTRACT2_BASE: u32 = 14 * CLASS_HASH_BASE;

// Contract names.
const ACCOUNT_LONG_VALIDATE_NAME: &str = "account_with_long_validate";
const ACCOUNT_WITHOUT_VALIDATIONS_NAME: &str = "account_with_dummy_validate";
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

const ACCOUNT_LONG_VALIDATE_COMPILED_CLASS_HASH_V1: expect_test::Expect =
    expect!["0x10a750f887ff1b9a48e323e882f4a46e60922c398aa3b661333aafb1dd4d408"];
const ACCOUNT_LONG_VALIDATE_COMPILED_CLASS_HASH_V2: expect_test::Expect =
    expect!["0x542acbb40fad0e9bf07b0d40ed72bfa3bea011a99d56ca568acdd9cf4360da4"];

const ACCOUNT_WITHOUT_VALIDATIONS_COMPILED_CLASS_HASH_V1: expect_test::Expect =
    expect!["0x4709fae1451c5a159b880f568d01ca623ce96e8943c7df91cf696940692b58"];
const ACCOUNT_WITHOUT_VALIDATIONS_COMPILED_CLASS_HASH_V2: expect_test::Expect =
    expect!["0x2fa9a4b44d4c9c0b5522b50fd0ec55fb78f1db356837e33f6ddda1cfe6e1b71"];

const CAIRO_STEPS_TEST_CONTRACT_COMPILED_CLASS_HASH_V1: expect_test::Expect =
    expect!["0x2224e95c2b03a6abb4b38abde3ae2288363ca49667c0b177d79991e48f01cd7"];
const CAIRO_STEPS_TEST_CONTRACT_COMPILED_CLASS_HASH_V2: expect_test::Expect =
    expect!["0x227ad31d8f8e64fa9fb25a5463f5010e384f3ce26214ba932807c8cf0a0ee0e"];

const EMPTY_ACCOUNT_COMPILED_CLASS_HASH_V1: expect_test::Expect =
    expect!["0x580eedf2308fdcd855257c118ad55a115ba2c13a04deefa9a5b39290a88f65b"];
const EMPTY_ACCOUNT_COMPILED_CLASS_HASH_V2: expect_test::Expect =
    expect!["0x63508c449d5e584fb0e4fac90e3cc6c46fbfbe8a7215e7f74b13391ab3a3071"];

const ERC20_COMPILED_CLASS_HASH_V1: expect_test::Expect =
    expect!["0x266f53b3f6cc2367c334b75ea86aff748ca27aa321019778af81be69d549159"];
const ERC20_COMPILED_CLASS_HASH_V2: expect_test::Expect =
    expect!["0x75a59a0cd1985dd66a3fd9d634f0e0287916cd8e0fff3f2bd80d69498b09367"];

const EMPTY_COMPILED_CLASS_HASH_V1: expect_test::Expect =
    expect!["0x317d3ac2cf840e487b6d0014a75f0cf507dff0bc143c710388e323487089bfa"];
const EMPTY_COMPILED_CLASS_HASH_V2: expect_test::Expect =
    expect!["0x6ee46561691e785d643a8296b9bf08008e432df405a1a4beb6ed784541b571c"];

const FAULTY_ACCOUNT_COMPILED_CLASS_HASH_V1: expect_test::Expect =
    expect!["0xfdb78bea47a0b1464f38b2d29997edac084bbed4e63e1eef240ffb86a34ab2"];
const FAULTY_ACCOUNT_COMPILED_CLASS_HASH_V2: expect_test::Expect =
    expect!["0x61b026bb31a602576477ad3b683aa3dd4080e3f845ab7f24ffc125a5dfc71bf"];

const LEGACY_CONTRACT_COMPILED_CLASS_HASH_V1: expect_test::Expect =
    expect!["0x1e9f18319ec0f9a4158522e9ccf356c08e9a074609b972a3b8fb2a8e49a2994"];
const LEGACY_CONTRACT_COMPILED_CLASS_HASH_V2: expect_test::Expect =
    expect!["0x6d9f03fde30543af9a457c4fcc13aced0ce27ef4e5a498a143d483b23711f32"];

const TEST_CONTRACT_COMPILED_CLASS_HASH_V1: expect_test::Expect =
    expect!["0x63e2ef597484dbb68c3112266f02b3aff66de1d49b85efd759bafd5375d5839"];
const TEST_CONTRACT_COMPILED_CLASS_HASH_V2: expect_test::Expect =
    expect!["0x6ccdcefdc050235d2512784e156714ac4c01fafbac9aac4ada6d99c068ae15d"];

const SIERRA_EXECUTION_INFO_V1_CONTRACT_COMPILED_CLASS_HASH_V1: expect_test::Expect =
    expect!["0x728a998871aed1335e5a7757119cdd88785e4ce9cc90bbf90075f39b34b22e5"];
const SIERRA_EXECUTION_INFO_V1_CONTRACT_COMPILED_CLASS_HASH_V2: expect_test::Expect =
    expect!["0x2d7880420d28543b7a71d3b90d786729170e494c92b9cf97e04b5a0c7ffbe49"];

const META_TX_CONTRACT_COMPILED_CLASS_HASH_V1: expect_test::Expect =
    expect!["0x7d8d6dab768dcdb34839588cb7e8279646a864c44755168d5c49c63cedf4fe8"];
const META_TX_CONTRACT_COMPILED_CLASS_HASH_V2: expect_test::Expect =
    expect!["0x6417ea9b788437bba0d1d34b725dfad6f4c0cbc6de13563b2a1647958c2e41f"];

const MOCK_STAKING_CONTRACT_COMPILED_CLASS_HASH_V1: expect_test::Expect =
    expect!["0x49a8fc93d796516a98d5517d6440ed71f479319f5b0aa786c9cb03440e84982"];
const MOCK_STAKING_CONTRACT_COMPILED_CLASS_HASH_V2: expect_test::Expect =
    expect!["0x3effc9574d4d25956524a806d0582a64a62fa8ec66f335366462ec670527724"];

pub type CairoVersionString = String;

/// Enum representing all feature contracts.
/// The contracts that are implemented in both Cairo versions include a version field.
#[derive(Clone, Copy, Debug, EnumIter, Hash, PartialEq, Eq)]
pub enum FeatureContract {
    AccountWithLongValidate(CairoVersion),
    AccountWithoutValidations(CairoVersion),
    DelegateProxy,
    EmptyAccount(RunnableCairo1),
    ERC20(CairoVersion),
    Empty(CairoVersion),
    FaultyAccount(CairoVersion),
    LegacyTestContract,
    SecurityTests,
    TestContract(CairoVersion),
    TestContract2,
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
            Self::DelegateProxy | Self::SecurityTests | Self::TestContract2 => CairoVersion::Cairo0,
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
            Self::DelegateProxy
            | Self::SecurityTests
            | Self::TestContract2
            | Self::CairoStepsTestContract
            | Self::LegacyTestContract => {
                panic!("{self:?} contract has no configurable version.")
            }
        }
    }

    pub fn get_class_hash(&self) -> ClassHash {
        class_hash!(self.get_integer_base())
    }

    // Returns (compiled_class_hash_v1, compiled_class_hash_v2)
    /// as consts Expect strings.
    pub fn get_compiled_class_hashes_constants(
        &self,
    ) -> (expect_test::Expect, expect_test::Expect) {
        match self {
            Self::AccountWithLongValidate(_) => (
                ACCOUNT_LONG_VALIDATE_COMPILED_CLASS_HASH_V1,
                ACCOUNT_LONG_VALIDATE_COMPILED_CLASS_HASH_V2,
            ),
            Self::AccountWithoutValidations(_) => (
                ACCOUNT_WITHOUT_VALIDATIONS_COMPILED_CLASS_HASH_V1,
                ACCOUNT_WITHOUT_VALIDATIONS_COMPILED_CLASS_HASH_V2,
            ),
            Self::Empty(_) => (EMPTY_COMPILED_CLASS_HASH_V1, EMPTY_COMPILED_CLASS_HASH_V2),
            Self::FaultyAccount(_) => {
                (FAULTY_ACCOUNT_COMPILED_CLASS_HASH_V1, FAULTY_ACCOUNT_COMPILED_CLASS_HASH_V2)
            }
            Self::LegacyTestContract => {
                (LEGACY_CONTRACT_COMPILED_CLASS_HASH_V1, LEGACY_CONTRACT_COMPILED_CLASS_HASH_V2)
            }
            Self::DelegateProxy | Self::SecurityTests | Self::TestContract2 => {
                panic!("{self:?} has no compiled class hash.")
            }
            Self::TestContract(_) => {
                (TEST_CONTRACT_COMPILED_CLASS_HASH_V1, TEST_CONTRACT_COMPILED_CLASS_HASH_V2)
            }
            Self::CairoStepsTestContract => (
                CAIRO_STEPS_TEST_CONTRACT_COMPILED_CLASS_HASH_V1,
                CAIRO_STEPS_TEST_CONTRACT_COMPILED_CLASS_HASH_V2,
            ),
            Self::SierraExecutionInfoV1Contract(_) => (
                SIERRA_EXECUTION_INFO_V1_CONTRACT_COMPILED_CLASS_HASH_V1,
                SIERRA_EXECUTION_INFO_V1_CONTRACT_COMPILED_CLASS_HASH_V2,
            ),
            Self::EmptyAccount(_) => {
                (EMPTY_ACCOUNT_COMPILED_CLASS_HASH_V1, EMPTY_ACCOUNT_COMPILED_CLASS_HASH_V2)
            }
            Self::MetaTx(_) => {
                (META_TX_CONTRACT_COMPILED_CLASS_HASH_V1, META_TX_CONTRACT_COMPILED_CLASS_HASH_V2)
            }
            Self::MockStakingContract(_) => (
                MOCK_STAKING_CONTRACT_COMPILED_CLASS_HASH_V1,
                MOCK_STAKING_CONTRACT_COMPILED_CLASS_HASH_V2,
            ),
            Self::ERC20(_) => (ERC20_COMPILED_CLASS_HASH_V1, ERC20_COMPILED_CLASS_HASH_V2),
        }
    }

    // Returns the contracts compiled class hash for the given hash version.
    pub fn get_compiled_class_hash(&self, hash_version: &HashVersion) -> CompiledClassHash {
        match self.cairo_version() {
            CairoVersion::Cairo0 => CompiledClassHash::default(),
            CairoVersion::Cairo1(_) => {
                let (casm_hash_v1, casm_hash_v2) = self.get_compiled_class_hashes_constants();
                match hash_version {
                    HashVersion::V1 => CompiledClassHash(felt!(casm_hash_v1.data)),
                    HashVersion::V2 => CompiledClassHash(felt!(casm_hash_v2.data)),
                }
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
                Self::DelegateProxy => DELEGATE_PROXY_BASE,
                Self::TestContract2 => TEST_CONTRACT2_BASE,
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
            Self::DelegateProxy => DELEGATE_PROXY_NAME,
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
                    FeatureContract::DelegateProxy
                    | FeatureContract::Empty(_)
                    | FeatureContract::TestContract(_)
                    | FeatureContract::TestContract2
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

            Self::DelegateProxy
            | Self::LegacyTestContract
            | Self::CairoStepsTestContract
            | Self::TestContract2
            | Self::SecurityTests => {
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
