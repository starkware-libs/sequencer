use std::collections::BTreeMap;
use std::fmt::{self, Display, Formatter};
use std::path::PathBuf;

use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::de::Error as DeserializeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use validator::Validate;

#[cfg(test)]
#[path = "config_test.rs"]
mod config_test;

// TODO(Noa): Reconsider the default values.
pub const DEFAULT_MAX_BYTECODE_SIZE: usize = 80 * 1024;
pub const DEFAULT_MAX_MEMORY_USAGE: u64 = 5 * 1024 * 1024 * 1024;
pub const DEFAULT_MAX_CPU_TIME: u64 = 60;
pub const DEFAULT_ALLOWED_LIBFUNCS_LIST: AllowedLibfuncsList = AllowedLibfuncsList::Audited;

/// Reserved names; any other string parses into [`AllowedLibfuncsList::File`].
const AUDITED_LIST_NAME: &str = "audited";
const ALL_LIST_NAME: &str = "all";
const BUNDLED_LIST_NAME: &str = "bundled";

/// Which libfuncs the Sierra-to-CASM compiler accepts.
///
/// Serialized as a single string, so it can be set through the flat deployment config.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AllowedLibfuncsList {
    /// The compiler's built-in audited list.
    Audited,
    /// The compiler's built-in list of every libfunc.
    All,
    /// The list bundled into this binary (`apollo_compile_to_casm/src/allowed_libfuncs.json`).
    Bundled,
    /// An operator-supplied list file.
    File(PathBuf),
}

impl Display for AllowedLibfuncsList {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Audited => write!(formatter, "{AUDITED_LIST_NAME}"),
            Self::All => write!(formatter, "{ALL_LIST_NAME}"),
            Self::Bundled => write!(formatter, "{BUNDLED_LIST_NAME}"),
            Self::File(path) => write!(formatter, "{}", path.display()),
        }
    }
}

impl From<&str> for AllowedLibfuncsList {
    fn from(value: &str) -> Self {
        match value {
            AUDITED_LIST_NAME => Self::Audited,
            ALL_LIST_NAME => Self::All,
            BUNDLED_LIST_NAME => Self::Bundled,
            path => Self::File(PathBuf::from(path)),
        }
    }
}

impl Serialize for AllowedLibfuncsList {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for AllowedLibfuncsList {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        // An unreserved value is taken as a file path, so reject the one shape that cannot be one
        // rather than deferring the failure to compiler startup.
        if value.is_empty() {
            return Err(D::Error::custom(
                "allowed_libfuncs_list must be 'audited', 'all', 'bundled', or a list file path",
            ));
        }
        Ok(Self::from(value.as_str()))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct SierraCompilationConfig {
    /// CASM bytecode size limit (in felts).
    pub max_bytecode_size: usize,
    /// Compilation process’s virtual memory (address space) byte limit.
    pub max_memory_usage: u64,
    /// Compilation process's CPU time limit (in seconds).
    pub max_cpu_time: u64,
    /// The libfunc list the compiler validates against.
    pub allowed_libfuncs_list: AllowedLibfuncsList,
}

impl Default for SierraCompilationConfig {
    fn default() -> Self {
        Self {
            max_bytecode_size: DEFAULT_MAX_BYTECODE_SIZE,
            max_memory_usage: DEFAULT_MAX_MEMORY_USAGE,
            max_cpu_time: DEFAULT_MAX_CPU_TIME,
            allowed_libfuncs_list: DEFAULT_ALLOWED_LIBFUNCS_LIST,
        }
    }
}

impl SerializeConfig for SierraCompilationConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([
            ser_param(
                "max_bytecode_size",
                &self.max_bytecode_size,
                "Limitation of compiled CASM bytecode size (felts).",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_cpu_time",
                &self.max_cpu_time,
                "Limitation of compilation cpu time (seconds).",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "allowed_libfuncs_list",
                &self.allowed_libfuncs_list,
                "The libfunc list to validate against: 'audited', 'all', 'bundled', or a path to \
                 a list file.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_memory_usage",
                &self.max_memory_usage,
                "Limitation of compilation process's virtual memory (bytes).",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}
