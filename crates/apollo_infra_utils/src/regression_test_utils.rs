use std::collections::{BTreeMap, HashSet};
use std::sync::{LazyLock, Mutex};

use serde::de::DeserializeOwned;
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
    values: BTreeMap<String, Value>,
}

impl MagicConstants {
    pub fn new(path: String, values: BTreeMap<String, Value>) -> Self {
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

    /// Fetches the value. In fix mode, use the provided "default".
    pub fn get<V: Default + DeserializeOwned>(
        &self,
        value_name: &'static str,
        fix_mode_value: V,
    ) -> V {
        if is_magic_fix_mode() {
            fix_mode_value
        } else {
            // In test mode, we return the value from the file.
            self.values
                .get(value_name)
                .and_then(|value| serde_json::from_value(value.clone()).ok())
                .unwrap_or_else(|| {
                    panic!("Magic constant {value_name} not found in file {}.", self.path)
                })
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
        let directory = std::path::PathBuf::from("magic_constants");

        // If we are in fix mode, and this is the first registration of a file in the current
        // directory, we need to delete all files in the directory (and possibly create the
        // directory) to keep the regression files "clean" (in case a file / test function was
        // renamed, we don't want to keep dangling JSON artifacts).
        if $crate::regression_test_utils::is_magic_fix_mode() {
            let locked_set =
                $crate::regression_test_utils::MAGIC_CONSTANTS_REGISTRY.0.lock().unwrap();
            let mut found = false;
            let current_dir = std::fs::canonicalize(".").unwrap_or_else(|error| {
                panic!("Failed to get absolute path to current location: {error}.")
            });
            for registered_path in locked_set.iter() {
                if registered_path.starts_with(current_dir.to_str().unwrap()) {
                    found = true;
                    break;
                }
            }
            if !found {
                // This is the first registration of a file in the current directory, so we need to
                // delete all files in the directory.
                // Create the directory if it doesn't exist.
                if directory.exists() {
                    for entry in std::fs::read_dir(&directory).unwrap() {
                        let entry = entry.unwrap();
                        let path = entry.path();
                        if path.is_file() {
                            std::fs::remove_file(&path).unwrap_or_else(|error| {
                                panic!(
                                    "Failed to remove magic constants file at {path:?}: {error}."
                                )
                            });
                        }
                    }
                } else {
                    std::fs::create_dir_all(&directory).unwrap_or_else(|error| {
                        panic!(
                            "Failed to create magic constants directory at {directory:?}: {error}."
                        )
                    });
                }
            }
        }
        let directory = std::fs::canonicalize(&directory).unwrap_or_else(|error| {
            panic!("Failed to get absolute path for magic constants directory: {error}.")
        });

        // Register the path.
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
        let mut values = std::collections::BTreeMap::new();
        if $crate::regression_test_utils::is_magic_fix_mode() {
            // In fix mode, we create a new file with the default values.
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
            values = std::collections::BTreeMap::from_iter(
                json.as_object().unwrap().clone().into_iter(),
            );
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
