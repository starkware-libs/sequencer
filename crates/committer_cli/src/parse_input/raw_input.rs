use committer::block_committer::input::ConfigImpl;
use log::LevelFilter;
use serde::{Deserialize, Serialize};
use serde_repr::Deserialize_repr;
type RawFelt = [u8; 32];

#[derive(Deserialize, Debug)]
/// Input to the committer.
pub(crate) struct RawInput {
    /// Storage. Will be casted to HashMap<vec<u8>, Vec<u8>> to simulate DB access.
    pub storage: Vec<RawStorageEntry>,
    pub state_diff: RawStateDiff,
    pub contracts_trie_root_hash: RawFelt,
    pub classes_trie_root_hash: RawFelt,
    pub config: RawConfigImpl,
}

#[derive(Deserialize, Debug)]
/// Fact storage entry.
pub(crate) struct RawStorageEntry {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}

#[derive(Deserialize, Debug)]
pub(crate) struct RawConfigImpl {
    warn_on_trivial_modifications: bool,
    log_level: PythonLogLevel,
}

#[derive(Deserialize_repr, Debug, Default, Serialize)]
#[repr(usize)]
/// Describes a log level https://docs.python.org/3/library/logging.html#logging-levels
pub(crate) enum PythonLogLevel {
    NotSet = 0,
    Info = 20,
    Warning = 30,
    Error = 40,
    Critical = 50,
    // If an unknown variant is given, the default log level is Debug.
    #[serde(other)]
    #[default]
    Debug = 10,
}

impl From<RawConfigImpl> for ConfigImpl {
    fn from(raw_config: RawConfigImpl) -> Self {
        let log_level = match raw_config.log_level {
            PythonLogLevel::NotSet => LevelFilter::Trace,
            PythonLogLevel::Debug => LevelFilter::Debug,
            PythonLogLevel::Info => LevelFilter::Info,
            PythonLogLevel::Warning => LevelFilter::Warn,
            PythonLogLevel::Error | PythonLogLevel::Critical => LevelFilter::Error,
        };
        ConfigImpl::new(raw_config.warn_on_trivial_modifications, log_level)
    }
}

#[derive(Deserialize, Debug)]
pub(crate) struct RawFeltMapEntry {
    pub key: RawFelt,
    pub value: RawFelt,
}

#[derive(Deserialize, Debug)]
/// Represents storage updates. Later will be casted to HashMap<Felt, HashMap<Felt,Felt>> entry.
pub(crate) struct RawStorageUpdates {
    pub address: RawFelt,
    pub storage_updates: Vec<RawFeltMapEntry>,
}

#[derive(Deserialize, Debug)]
/// Represents state diff.
pub(crate) struct RawStateDiff {
    /// Will be casted to HashMap<Felt, Felt>.
    pub address_to_class_hash: Vec<RawFeltMapEntry>,
    /// Will be casted to HashMap<Felt, Felt>.
    pub address_to_nonce: Vec<RawFeltMapEntry>,
    /// Will be casted to HashMap<Felt, Felt>.
    pub class_hash_to_compiled_class_hash: Vec<RawFeltMapEntry>,
    /// Will be casted to HashMap<Felt, HashMap<Felt, Felt>>.
    pub storage_updates: Vec<RawStorageUpdates>,
}
