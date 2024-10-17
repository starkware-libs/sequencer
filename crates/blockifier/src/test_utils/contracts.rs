use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
#[cfg(feature = "cairo_native")]
use cairo_lang_starknet_classes::contract_class::ContractClass as SierraContractClass;
use starknet_api::contract_class::{ContractClass, EntryPointType};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, EntryPointSelector};
use starknet_api::deprecated_contract_class::{
    ContractClass as DeprecatedContractClass,
    EntryPointOffset,
};
use starknet_api::{class_hash, contract_address, felt};
use starknet_types_core::felt::Felt;
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

use crate::abi::abi_utils::selector_from_name;
use crate::abi::constants::CONSTRUCTOR_ENTRY_POINT_NAME;
use crate::execution::contract_class::RunnableContractClass;
use crate::execution::entry_point::CallEntryPoint;
#[cfg(feature = "cairo_native")]
use crate::test_utils::cairo_compile::starknet_compile;
use crate::test_utils::cairo_compile::{cairo0_compile, cairo1_compile};
use crate::test_utils::struct_impls::LoadContractFromFile;
use crate::test_utils::{get_raw_contract_class, CairoVersion};

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

// Contract names.
const ACCOUNT_LONG_VALIDATE_NAME: &str = "account_with_long_validate";
const ACCOUNT_WITHOUT_VALIDATIONS_NAME: &str = "account_with_dummy_validate";
const EMPTY_CONTRACT_NAME: &str = "empty_contract";
const FAULTY_ACCOUNT_NAME: &str = "account_faulty";
const LEGACY_CONTRACT_NAME: &str = "legacy_test_contract";
const SECURITY_TEST_CONTRACT_NAME: &str = "security_tests_contract";
const TEST_CONTRACT_NAME: &str = "test_contract";
const CAIRO_STEPS_TEST_CONTRACT_NAME: &str = "cairo_steps_test_contract";

// ERC20 contract is in a unique location.
const ERC20_CAIRO0_CONTRACT_SOURCE_PATH: &str =
    "./ERC20/ERC20_Cairo0/ERC20_without_some_syscalls/ERC20/ERC20.cairo";
const ERC20_CAIRO0_CONTRACT_PATH: &str = "./ERC20/ERC20_Cairo0/ERC20_without_some_syscalls/ERC20/\
                                          erc20_contract_without_some_syscalls_compiled.json";
const ERC20_CAIRO1_CONTRACT_SOURCE_PATH: &str = "./ERC20/ERC20_Cairo1/ERC20.cairo";
const ERC20_CAIRO1_CONTRACT_PATH: &str = "./ERC20/ERC20_Cairo1/erc20.casm.json";

// The following contracts are compiled with a fixed version of the compiler. This compiler version
// no longer compiles with stable rust, so the toolchain is also fixed.
const LEGACY_CONTRACT_COMPILER_TAG: &str = "v2.1.0";
const LEGACY_CONTRACT_RUST_TOOLCHAIN: &str = "2023-07-05";

const CAIRO_STEPS_TEST_CONTRACT_COMPILER_TAG: &str = "v2.7.0";
const CAIRO_STEPS_TEST_CONTRACT_RUST_TOOLCHAIN: &str = "2024-04-29";

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
            Self::LegacyTestContract | Self::CairoStepsTestContract => CairoVersion::Cairo1,
        }
    }

    /// Returns a bit mask of supported Cairo versions for the feature contract.
    ///
    /// Each bit identifies a supported Cairo version. The least significant bit is Cairo0, the next
    /// bit is Cairo1, and the most significant bit is the native version.
    ///
    /// This order is defined in [CairoVersion] enum.
    fn supported_versions(&self) -> u32 {
        let supports_legacy = matches!(
            self,
            Self::FaultyAccount(_)
                | Self::AccountWithoutValidations(_)
                | Self::AccountWithLongValidate(_)
                | Self::Empty(_)
                | Self::TestContract(_)
                | Self::SecurityTests
                | Self::ERC20(_)
        );

        let supports_cairo1 = matches!(
            self,
            Self::FaultyAccount(_)
                | Self::AccountWithoutValidations(_)
                | Self::AccountWithLongValidate(_)
                | Self::Empty(_)
                | Self::LegacyTestContract
                | Self::TestContract(_)
                | Self::ERC20(_)
                | Self::CairoStepsTestContract
        );

        let supports_native = matches!(self, Self::TestContract(_));

        (u32::from(supports_legacy))
            | (u32::from(supports_cairo1)) << 1
            | (u32::from(supports_native)) << 2
    }

    pub fn set_cairo_version(&mut self, version: CairoVersion) {
        match self {
            Self::AccountWithLongValidate(v)
            | Self::AccountWithoutValidations(v)
            | Self::Empty(v)
            | Self::FaultyAccount(v)
            | Self::TestContract(v)
            | Self::ERC20(v) => *v = version,
            Self::LegacyTestContract | Self::SecurityTests | Self::CairoStepsTestContract => {
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
            CairoVersion::Cairo1 => CompiledClassHash(felt!(self.get_integer_base())),
            #[cfg(feature = "cairo_native")]
            CairoVersion::Native => CompiledClassHash(felt!(self.get_integer_base())),
        }
    }

    /// Returns the address of the instance with the given instance ID.
    pub fn get_instance_address(&self, instance_id: u16) -> ContractAddress {
        let instance_id_as_u32: u32 = instance_id.into();
        contract_address!(self.get_integer_base() + instance_id_as_u32 + ADDRESS_BIT)
    }

    pub fn get_class(&self) -> ContractClass {
        match self.cairo_version() {
            CairoVersion::Cairo0 => {
                ContractClass::V0(DeprecatedContractClass::from_file(&self.get_compiled_path()))
            }
            CairoVersion::Cairo1 => {
                ContractClass::V1(CasmContractClass::from_file(&self.get_compiled_path()))
            }
            #[cfg(feature = "cairo_native")]
            CairoVersion::Native => {
                ContractClass::V1Native(SierraContractClass::from_file(&self.get_compiled_path()))
            }
        }
    }

    pub fn get_runnable_class(&self) -> RunnableContractClass {
        self.get_class().try_into().unwrap()
    }

    // TODO(Arni, 1/1/2025): Remove this function, and use the get_class function instead.
    pub fn get_deprecated_contract_class(&self) -> DeprecatedContractClass {
        let mut raw_contract_class: serde_json::Value =
            serde_json::from_str(&self.get_raw_class()).unwrap();

        // ABI is not required for execution.
        raw_contract_class
            .as_object_mut()
            .expect("A compiled contract must be a JSON object.")
            .remove("abi");

        serde_json::from_value(raw_contract_class)
            .expect("DeprecatedContractClass is not supported for this contract.")
    }

    pub fn get_raw_class(&self) -> String {
        get_raw_contract_class(&self.get_compiled_path())
    }

    fn get_cairo_version_bit(&self) -> u32 {
        match self.cairo_version() {
            CairoVersion::Cairo0 => 0,
            CairoVersion::Cairo1 => CAIRO1_BIT,
            #[cfg(feature = "cairo_native")]
            CairoVersion::Native => CAIRO1_BIT,
        }
    }

    /// Some contracts are designed to test behavior of code compiled with a
    /// specific (old) compiler tag. To run the (old) compiler, older rust
    /// version is required.
    pub fn fixed_tag_and_rust_toolchain(&self) -> (Option<String>, Option<String>) {
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
            Self::ERC20(_) => unreachable!(),
        }
    }

    pub fn get_source_path(&self) -> String {
        // Special case: ERC20 contract in a different location.
        if let Self::ERC20(cairo_version) = self {
            match cairo_version {
                CairoVersion::Cairo0 => ERC20_CAIRO0_CONTRACT_SOURCE_PATH,
                CairoVersion::Cairo1 => ERC20_CAIRO1_CONTRACT_SOURCE_PATH,
                #[cfg(feature = "cairo_native")]
                CairoVersion::Native => todo!("ERC20 cannot be tested with Native"),
            }
            .into()
        } else {
            format!(
                "feature_contracts/cairo{}/{}.cairo",
                match self.cairo_version() {
                    CairoVersion::Cairo0 => "0",
                    CairoVersion::Cairo1 => "1",
                    #[cfg(feature = "cairo_native")]
                    CairoVersion::Native => "1",
                },
                self.get_non_erc20_base_name()
            )
        }
    }

    pub fn get_compiled_path(&self) -> String {
        // ERC20 is a special case - not in the feature_contracts directory.
        if let Self::ERC20(cairo_version) = self {
            match cairo_version {
                CairoVersion::Cairo0 => ERC20_CAIRO0_CONTRACT_PATH,
                CairoVersion::Cairo1 => ERC20_CAIRO1_CONTRACT_PATH,
                #[cfg(feature = "cairo_native")]
                CairoVersion::Native => todo!("ERC20 cannot be tested with Native"),
            }
            .into()
        } else {
            let cairo_version = self.cairo_version();
            format!(
                "feature_contracts/cairo{}/compiled{}/{}{}.json",
                match cairo_version {
                    CairoVersion::Cairo0 => "0",
                    CairoVersion::Cairo1 => "1",
                    #[cfg(feature = "cairo_native")]
                    CairoVersion::Native => "1",
                },
                match self.cairo_version() {
                    CairoVersion::Cairo0 => "",
                    CairoVersion::Cairo1 => "_casm",
                    #[cfg(feature = "cairo_native")]
                    CairoVersion::Native => "_sierra",
                },
                self.get_non_erc20_base_name(),
                match cairo_version {
                    CairoVersion::Cairo0 => "_compiled",
                    CairoVersion::Cairo1 => ".casm",
                    #[cfg(feature = "cairo_native")]
                    CairoVersion::Native => ".sierra",
                }
            )
        }
    }

    /// Compiles the feature contract and returns the compiled contract as a byte vector.
    /// Panics if the contract is ERC20, as ERC20 contract recompilation is not supported.
    pub fn compile(&self) -> Vec<u8> {
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
                    | FeatureContract::CairoStepsTestContract => None,
                    FeatureContract::ERC20(_) => unreachable!(),
                };
                cairo0_compile(self.get_source_path(), extra_arg, false)
            }
            CairoVersion::Cairo1 => {
                let (tag_override, cargo_nightly_arg) = self.fixed_tag_and_rust_toolchain();
                cairo1_compile(self.get_source_path(), tag_override, cargo_nightly_arg)
            }
            #[cfg(feature = "cairo_native")]
            CairoVersion::Native => {
                let (tag_override, cargo_nightly_arg) = self.fixed_tag_and_rust_toolchain();
                starknet_compile(self.get_source_path(), tag_override, cargo_nightly_arg)
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
            RunnableContractClass::V0(class) => {
                class
                    .entry_points_by_type
                    .get(&entry_point_type)
                    .unwrap()
                    .iter()
                    .find(|ep| ep.selector == entry_point_selector)
                    .unwrap()
                    .offset
            }
            RunnableContractClass::V1(class) => {
                class
                    .entry_points_by_type
                    .get_entry_point(&CallEntryPoint {
                        entry_point_type,
                        entry_point_selector,
                        ..Default::default()
                    })
                    .unwrap()
                    .offset
            }
            #[cfg(feature = "cairo_native")]
            RunnableContractClass::V1Native(_) => {
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

    pub fn all_contracts() -> impl Iterator<Item = Self> {
        // EnumIter iterates over all variants with Default::default() as the cairo
        // version.
        Self::iter().flat_map(|contract| {
            // If only one supported version exists, add the contract to the array as is.
            if contract.supported_versions().is_power_of_two() {
                vec![contract].into_iter()
            } else {
                let supported_versions = contract.supported_versions();

                #[cfg(feature = "cairo_native")]
                let range = 0..3isize;
                #[cfg(not(feature = "cairo_native"))]
                let range = 0..2isize;

                // If multiple supported versions exist, add each supported version of the
                // contract to the array.
                range
                    .filter(|i| supported_versions & (1u32 << i) != 0)
                    .map(move |i| {
                        let mut contract = contract;
                        contract.set_cairo_version(CairoVersion::from(i));

                        contract
                    })
                    .collect::<Vec<_>>()
                    .into_iter()
            }
        })
    }

    pub fn all_feature_contracts() -> impl Iterator<Item = Self> {
        // ERC20 is a special case - not in the feature_contracts directory.
        Self::all_contracts().filter(|contract| !matches!(contract, Self::ERC20(_)))
    }
}
