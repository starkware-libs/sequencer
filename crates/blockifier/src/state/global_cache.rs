use std::sync::Arc;

use starknet_api::class_cache::GlobalContractCache;
use starknet_api::contract_class::ContractClass;
use starknet_api::state::SierraContractClass;

use crate::execution::contract_class::{CompiledClassV0, CompiledClassV1, RunnableCompiledClass};
#[cfg(feature = "cairo_native")]
use crate::execution::native::contract_class::NativeCompiledClassV1;
use crate::state::state_api::StateResult;

pub const GLOBAL_CONTRACT_CACHE_SIZE_FOR_TEST: usize = 600;

#[derive(Debug, Clone)]
pub enum CompiledClasses {
    V0(CompiledClassV0),
    V1(CompiledClassV1, Arc<SierraContractClass>),
    #[cfg(feature = "cairo_native")]
    V1Native(CachedCairoNative),
}
impl CompiledClasses {
    pub fn to_runnable(&self) -> RunnableCompiledClass {
        match self {
            CompiledClasses::V0(compiled_class_v0) => {
                RunnableCompiledClass::V0(compiled_class_v0.clone())
            }
            CompiledClasses::V1(compiled_class_v1, _sierra_contract_class) => {
                RunnableCompiledClass::V1(compiled_class_v1.clone())
            }
            #[cfg(feature = "cairo_native")]
            CompiledClasses::V1Native(cached_cairo_native) => match cached_cairo_native {
                CachedCairoNative::Compiled(native_compiled_class_v1) => {
                    RunnableCompiledClass::V1Native(native_compiled_class_v1.clone())
                }
                CachedCairoNative::CompilationFailed(compiled_class_v1) => {
                    RunnableCompiledClass::V1(compiled_class_v1.clone())
                }
            },
        }
    }

    /// Create a CompiledClasses object from a ContractClass.
    /// If the ContractClass is a Cairo 1 contract class, the sierra contract class is required
    /// as CompiledClasses::V1 objects contains a SierraContractClass.
    pub fn from_contract_class(
        contract_class: &ContractClass,
        sierra_contract_class: Option<SierraContractClass>,
    ) -> StateResult<CompiledClasses> {
        match contract_class {
            ContractClass::V0(deprecated_class) => {
                Ok(CompiledClasses::V0(CompiledClassV0::try_from(deprecated_class.clone())?))
            }
            ContractClass::V1(versioned_casm) => {
                let sierra_contract_class =
                    if let Some(sierra_contract_class) = sierra_contract_class {
                        sierra_contract_class
                    } else {
                        panic!("Expected Sierra contract class, got None");
                    };
                Ok(CompiledClasses::V1(
                    CompiledClassV1::try_from(versioned_casm.clone())?,
                    Arc::new(sierra_contract_class.clone()),
                ))
            }
        }
    }
}

pub type RawClassCache = GlobalContractCache<CompiledClasses>;

#[cfg(feature = "cairo_native")]
#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq))]
pub enum CachedCairoNative {
    Compiled(NativeCompiledClassV1),
    CompilationFailed(CompiledClassV1),
}
