use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::abi::constants::CONSTRUCTOR_ENTRY_POINT_NAME;
use starknet_api::contract_class::compiled_class_hash::{HashVersion, HashableCompiledClass};
use starknet_api::contract_class::{ContractClass, EntryPointType};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, EntryPointSelector};
use starknet_api::deprecated_contract_class::{
    ContractClass as DeprecatedContractClass,
    EntryPointOffset,
};
use starknet_api::state::SierraContractClass;

use crate::execution::contract_class::RunnableCompiledClass;
use crate::execution::entry_point::EntryPointTypeAndSelector;
#[cfg(feature = "cairo_native")]
use crate::execution::native::contract_class::NativeCompiledClassV1;
use crate::test_utils::struct_impls::LoadContractFromFile;

pub trait FeatureContractTrait {
    fn get_class(&self) -> ContractClass;
    fn get_runnable_class(&self) -> RunnableCompiledClass;

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

    fn get_entry_point_offset(&self, entry_point_selector: EntryPointSelector) -> EntryPointOffset {
        self.get_offset(entry_point_selector, EntryPointType::External)
    }

    fn get_ctor_offset(
        &self,
        entry_point_selector: Option<EntryPointSelector>,
    ) -> EntryPointOffset {
        let selector =
            entry_point_selector.unwrap_or(selector_from_name(CONSTRUCTOR_ENTRY_POINT_NAME));
        self.get_offset(selector, EntryPointType::Constructor)
    }

    fn get_compiled_class_hash_v2(&self) -> CompiledClassHash {
        match self.get_runnable_class() {
            RunnableCompiledClass::V0(_) => CompiledClassHash::default(),
            RunnableCompiledClass::V1(class) => class.hash(&HashVersion::V2),
            #[cfg(feature = "cairo_native")]
            RunnableCompiledClass::V1Native(class) => class.hash(&HashVersion::V2),
        }
    }
}

impl FeatureContractTrait for FeatureContract {
    fn get_class(&self) -> ContractClass {
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

    fn get_runnable_class(&self) -> RunnableCompiledClass {
        #[cfg(feature = "cairo_native")]
        if CairoVersion::Cairo1(RunnableCairo1::Native) == self.cairo_version() {
            let native_contract_class =
                NativeCompiledClassV1::compile_or_get_cached(&self.get_compiled_path());
            return RunnableCompiledClass::V1Native(native_contract_class);
        }

        self.get_class().try_into().unwrap()
    }
}

/// The information needed to test a [FeatureContract].
pub struct FeatureContractData {
    pub class_hash: ClassHash,
    pub runnable_class: RunnableCompiledClass,
    pub sierra: Option<SierraContractClass>,
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
            sierra: contract.safe_get_sierra(),
        }
    }
}
