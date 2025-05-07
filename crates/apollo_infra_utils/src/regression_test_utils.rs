use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex};

use serde::Serialize;
use serde_json::Value;

pub static MAGIC_CONSTANTS_REGISTRY: LazyLock<MagicConstantsRegistry> =
    LazyLock::new(|| MagicConstantsRegistry::default());

#[derive(Default)]
pub struct MagicConstantsRegistry(pub Mutex<HashSet<String>>);

pub fn is_magic_fix_mode() -> bool {
    std::env::var("MAGIC_FIX").is_ok()
}

pub struct MagicConstants {
    path: String,
    values: HashMap<String, Value>,
}

impl MagicConstants {
    pub fn new(path: String, values: HashMap<String, Value>) -> Self {
        Self { path, values }
    }

    #[track_caller]
    pub fn assert_eq<V: Serialize>(&mut self, value_name: &'static str, value: V) {
        if is_magic_fix_mode() {
            // In fix mode, we just set the value in the file.
            self.values.insert(value_name.to_string(), serde_json::to_value(value).unwrap());
        } else {
            let expected = self.values.get(value_name).unwrap_or_else(|| {
                panic!("Magic constant {value_name} not found in file {}.", self.path)
            });
            let actual: Value = serde_json::to_value(value).unwrap();
            assert_eq!(expected, &actual);
        }
    }
}

impl Drop for MagicConstants {
    // In fix mode: dump the values to the file on drop.
    fn drop(&mut self) {
        if is_magic_fix_mode() {
            std::fs::write(&self.path, serde_json::to_string_pretty(&self.values).unwrap())
                .unwrap_or_else(|error| {
                    panic!("Failed to write magic constants contents to {}: {}", self.path, error)
                });
        }
    }
}

/// Macro to output the fully qualified name of the function in which it's called.
/// Used to create unique names for the magic constants files in different functions.
#[macro_export]
macro_rules! function_name {
    () => {{
        fn f() {}
        fn type_name_of<T>(_: T) -> &'static str {
            std::any::type_name::<T>()
        }
        let name = type_name_of(f);
        name.strip_suffix("::f").unwrap()
    }};
}

#[macro_export]
macro_rules! register_magic_constants {
    ($unique_name:expr) => {{
        // Register the path.
        let directory = std::path::PathBuf::from("magic_constants");
        let path = directory
            .join(format!("{}_{}.json", $crate::function_name!(), $unique_name))
            .to_str()
            .unwrap()
            .to_string();
        if !$crate::regression_test_utils::MAGIC_CONSTANTS_REGISTRY
            .0
            .lock()
            .unwrap()
            .insert(path.clone())
        {
            panic!("Magic constants file already registered: {path}");
        }

        // TODO(Dori): Cleanup the magic_constants directory, if this is the first registration of
        //   a constants file in the current directory + we are in fix mode (to cleanup constants).
        //   Note that the lock on the registry will need to be taken explicitly for this.

        // Load / recreate the file, depending on the mode.
        let mut values = std::collections::HashMap::new();
        if $crate::regression_test_utils::is_magic_fix_mode() {
            // In fix mode, we create a new file with the default values.
            // Note that if the directory does not exist - we need to lock the registry before
            // creating it, to prevent races.
            if !directory.exists() {
                let _lock = $crate::regression_test_utils::MAGIC_CONSTANTS_REGISTRY.0.lock();
                std::fs::create_dir_all(&directory).unwrap_or_else(|error| {
                    panic!("Failed to create magic constants directory at {directory:?}: {error}.")
                });
            }
            let file = std::fs::File::create(&path).unwrap_or_else(|error| {
                panic!("Failed to create magic constants file at {path}: {error}.")
            });
            let writer = std::io::BufWriter::new(file);
            serde_json::to_writer(writer, &values).unwrap_or_else(|error| {
                panic!("Failed to write magic constants contents to {path}: {error}.")
            });
        } else {
            // In test mode, we load the file and return the values.
            let file = std::fs::File::open(&path).unwrap_or_else(|error| {
                panic!("Failed to open magic constants file at {path}: {error}.")
            });
            let reader = std::io::BufReader::new(file);
            let json: serde_json::Value = serde_json::from_reader(reader).unwrap();
            values =
                std::collections::HashMap::from_iter(json.as_object().unwrap().clone().into_iter());
        }

        let absolute_path = std::fs::canonicalize(&path).unwrap_or_else(|error| {
            panic!("Failed to get absolute path for magic constants file at {path}: {error}.")
        });
        $crate::regression_test_utils::MagicConstants::new(
            absolute_path.to_str().unwrap().to_string(),
            values,
        )
    }};
}
