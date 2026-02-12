use std::sync::Arc;

use starknet_api::class_cache::GlobalContractCache;
use starknet_api::state::SierraContractClass;

use crate::execution::contract_class::{CompiledClassV0, CompiledClassV1, RunnableCompiledClass};
#[cfg(feature = "cairo_native")]
use crate::execution::native::contract_class::NativeCompiledClassV1;

pub const GLOBAL_CONTRACT_CACHE_SIZE_FOR_TEST: usize = 600;

#[derive(Debug, Clone)]
#[cfg_attr(any(feature = "testing", test), derive(PartialEq))]
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

    // Note: For Cairo 1 classes, the Sierra does not match the Casm.
    #[cfg(any(feature = "testing", test))]
    pub fn from_runnable_for_testing(runnable_compiled_class: RunnableCompiledClass) -> Self {
        match runnable_compiled_class {
            RunnableCompiledClass::V0(compiled_class_v0) => Self::V0(compiled_class_v0),
            RunnableCompiledClass::V1(compiled_class_v1) => {
                Self::V1(compiled_class_v1, Arc::new(SierraContractClass::default()))
            }
            #[cfg(feature = "cairo_native")]
            RunnableCompiledClass::V1Native(native_compiled_class_v1) => {
                Self::V1Native(CachedCairoNative::Compiled(native_compiled_class_v1))
            }
        }
    }

    #[cfg(feature = "cairo_native")]
    /// Converts a [CompiledClasses::V1Native] to a [CompiledClasses::V1].
    /// Used when a non-native class is requested and the class is cached as a native class.
    pub fn v1_native_to_v1(self) -> CompiledClasses {
        match self {
            CompiledClasses::V1Native(CachedCairoNative::Compiled(native)) => {
                CompiledClasses::V1(native.casm(), Arc::new(SierraContractClass::default()))
            }
            _ => self,
        }
    }
}

pub type RawClassCache = GlobalContractCache<CompiledClasses>;

#[cfg(feature = "cairo_native")]
#[derive(Debug, Clone)]
#[cfg_attr(any(feature = "testing", test), derive(PartialEq))]
pub enum CachedCairoNative {
    Compiled(NativeCompiledClassV1),
    CompilationFailed(CompiledClassV1),
}
