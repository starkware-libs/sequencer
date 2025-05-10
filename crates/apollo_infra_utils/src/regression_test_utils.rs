/// Utilities for regression tests with "magic" values: values that are tested against computed
/// values, and are stored in JSON files.
/// See the `register_magic_constants!` macro docstring for more details and examples.
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use serde::Serialize;
use serde_json::Value;

#[cfg(test)]
#[path = "regression_test_utils_test.rs"]
mod regression_test_utils_test;

/// Global registry (lock) for magic constants files. Used to keep track of the "magic number" files
/// that are generated / used by regression tests, and control access to them.
#[derive(Default)]
struct MagicConstantsRegistry(pub Mutex<HashSet<String>>);

static MAGIC_CONSTANTS_REGISTRY: LazyLock<MagicConstantsRegistry> =
    LazyLock::new(MagicConstantsRegistry::default);

/// Check if we are in "clean" mode. In this mode, we delete all files in the magic constants
/// directory before creating a new one. This is used to keep the regression files "clean" (in case
/// a file / test function was renamed, we don't want to keep dangling JSON artifacts).
fn is_magic_clean_fix_mode() -> bool {
    std::env::var("MAGIC_CLEAN_FIX").is_ok()
}

/// Check if we are in "fix" mode. In this mode, we create a new file with the default values.
fn is_magic_fix_mode() -> bool {
    is_magic_clean_fix_mode() || std::env::var("MAGIC_FIX").is_ok()
}

/// Struct to hold the magic constants values. The values are stored in a BTreeMap, to keep the key
/// order deterministic. The values are generic serializable objects, so we can store any type of
/// value, as long as it's serializable.
pub struct MagicConstants {
    path: String,
    values: BTreeMap<String, Value>,
}

impl MagicConstants {
    /// Should not be called explicitly; use the `register_magic_constants!` macro instead.
    fn new(path: String, values: BTreeMap<String, Value>) -> Self {
        Self { path, values }
    }

    /// Main function to assert the equality of a value with the one in the file.
    /// If you have a test that uses a magic constant, you should use this function to assert the
    /// equality of the value.
    /// See docstring of `register_magic_constants!` macro for more details.
    #[track_caller]
    pub fn assert_eq<V: Serialize>(&mut self, value_name: &str, value: V) {
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

/// TAKES THE LOCK.
/// In fix mode, automatically dump the values to the file on drop (when test ends).
/// Checks if the file exists first - if it does, the existing values are loaded and the current
/// dict is updated, before dumping the contents.
impl Drop for MagicConstants {
    fn drop(&mut self) {
        if is_magic_fix_mode() {
            let _lock = MAGIC_CONSTANTS_REGISTRY.0.lock().unwrap();
            if PathBuf::from(&self.path).exists() {
                // If the file exists, we need to load the existing values and update them.
                let file = std::fs::File::open(&self.path).unwrap_or_else(|error| {
                    panic!("Failed to open magic constants file at {}: {}", self.path, error)
                });
                let reader = std::io::BufReader::new(file);
                let json: serde_json::Value = serde_json::from_reader(reader).unwrap();
                let values = BTreeMap::from_iter(json.as_object().unwrap().clone());
                self.values.extend(values);
            }
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

/// TAKES THE LOCK, if clean mode is active.
/// If we are in CLEAN mode, and this is the first registration of a file in the current
/// directory, we need to delete all files in the directory (and possibly create the
/// directory) to keep the regression files "clean" (in case a file / test function was
/// renamed, we don't want to keep dangling JSON artifacts).
fn clean_if_first_registration(current_dir: &Path, magic_subdir: &PathBuf) {
    if !is_magic_clean_fix_mode() {
        return;
    }
    let locked_set = MAGIC_CONSTANTS_REGISTRY.0.lock().unwrap();
    for registered_path in locked_set.iter() {
        if registered_path.starts_with(current_dir.to_str().unwrap()) {
            // Already registered a file in this directory, so we don't need to clean it.
            return;
        }
    }

    // This is the first registration of a file in the current magic_subdir, so we need to
    // delete all files in the magic_subdir.
    // Create the magic_subdir if it doesn't exist.
    if magic_subdir.exists() {
        for entry in std::fs::read_dir(magic_subdir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_file() {
                std::fs::remove_file(&path).unwrap_or_else(|error| {
                    panic!("Failed to remove magic constants file at {path:?}: {error}.")
                });
            }
        }
    } else {
        std::fs::create_dir_all(magic_subdir).unwrap_or_else(|error| {
            panic!("Failed to create magic constants directory at {magic_subdir:?}: {error}.")
        });
    }
}

/// TAKES THE LOCK.
/// Given the absolute path to the magic constants directory, and the identifiers required to
/// generate the specific JSON filename, loads and returns the `MagicConstants` object.
/// If the file does not exist, it is created with an empty dict (regardless of run mode).
fn load_magic_constants(directory: &Path, function_name: &str) -> MagicConstants {
    let mut locked = MAGIC_CONSTANTS_REGISTRY.0.lock().unwrap();

    // Compute the absolute path, and register it.
    let bad_chars = regex::Regex::new(r"[:()\[\]]").unwrap();
    let magic_filename = bad_chars.replace_all(&format!("{function_name}.json"), "_").to_string();
    let absolute_path = directory.join(magic_filename).to_str().unwrap().to_string();
    locked.insert(absolute_path.clone());

    // If the file doesn't exist, create it with an empty object.
    if !PathBuf::from(&absolute_path).exists() {
        if !directory.exists() {
            std::fs::create_dir_all(directory).unwrap_or_else(|error| {
                panic!("Failed to create magic constants directory at {directory:?}: {error}.")
            });
        }
        std::fs::File::create(&absolute_path).unwrap_or_else(|error| {
            panic!("Failed to create magic constants file at {absolute_path}: {error}.")
        });
        std::fs::write(&absolute_path, "{}").unwrap_or_else(|error| {
            panic!("Failed to write empty dict to {absolute_path}: {error}.")
        });
    }

    let file = std::fs::File::open(&absolute_path).unwrap_or_else(|error| {
        panic!("Failed to open magic constants file at {absolute_path:?}: {error}.")
    });
    let reader = std::io::BufReader::new(file);
    let json: serde_json::Value = serde_json::from_reader(reader).unwrap();
    let values = BTreeMap::from_iter(json.as_object().unwrap().clone());
    MagicConstants::new(absolute_path, values)
}

/// TAKES THE LOCK.
/// For documentation, see `register_magic_constants!` macro.
pub fn register_magic_constants_logic(current_dir: &Path, function_name: &str) -> MagicConstants {
    let directory = current_dir.join("magic_constants");
    clean_if_first_registration(current_dir, &directory);
    load_magic_constants(&directory, function_name)
}

/// Main logic of this module. Used to register and initialize the magic constants for a specific
/// test.
/// Each registration corresponds to a JSON file in the `magic_constants` directory of the calling
/// crate.
/// The same file will notbe generated twice - the filename is always unique per test function,
/// however, parametrized tests may use the same filename for different test cases.
///
/// For example, the old way of doing things looks something like this:
/// ```rust
/// fn test_something() {
///     let computation_result = 3 + 4;
///     assert_eq!(computation_result, 7);
/// }
/// ```
///
/// To use the new method, you need to add the `register_magic_constants!` macro to the test, and
/// assert using the `MagicConstants` object:
/// ```rust
/// # #[macro_use] extern crate apollo_infra_utils;
/// fn test_something() {
///     let mut magic = register_magic_constants!();
///     let computation_result = 3 + 4;
///     magic.assert_eq("MY_VALUE", computation_result);
/// }
/// ```
///
/// Then, generate the JSON file with the computed values by running:
/// ```bash
/// MAGIC_FIX=1 cargo test -p <MY_CRATE> test_something
/// ```
///
/// This will create a JSON file in the `magic_constants` directory of the calling crate, with the
/// dict `{ "MY_VALUE": 7 }`.
///
/// For parametrized tests, you can make the key include the parameter(s) to generate different
/// expected values for the different cases. For example:
/// ```rust
/// # #[macro_use] extern crate apollo_infra_utils;
/// #[rstest::rstest]
/// fn test_something(#[values(1, 2)] value: u32) {
///     let mut magic = register_magic_constants!();
///     let computation_result = value + 6;
///     magic.assert_eq(format!("MY_VALUE_{value}"), computation_result);
/// }
/// ```
///
/// This will generate two separate keys in the JSON file, one per test case.
/// The expected values in each test case may be identical, or different, but the keys will be
/// unique.
///
/// On the other hand, if you want to assert that the different parameters result in the same
/// regression values, you can use the same key for different test cases. For example, if the
/// regression value depends on `x` but not on `y`, you can do the following:
/// ```rust
/// # #[macro_use] extern crate apollo_infra_utils;
/// #[rstest::rstest]
/// fn test_something(#[values(1, 2)] x: u32, #[values(3, 4)] y: u32) {
///     let mut magic = register_magic_constants!();
///     let computation_result = x + 6;
///     magic.assert_eq(format!("MY_VALUE_FOR_X_{x}"), computation_result);
/// }
/// ```
///
/// The macro behaves differently depending on the mode:
/// 1. If vanilla `cargo test` is run (no fix / clean modes), it will load the values from the file.
///    If the file does not exist, it will panic.
/// 2. If we are in fix mode, but not clean mode, new files will be created (with an empty object).
///    Note that this will not delete any existing files, unless the name is identical. See
///    `is_magic_fix_mode` for how to activate this mode.
/// 3. If we are in clean mode, all files in the `magic_constants` directory of the calling crate
///    will be deleted before new files are generated. This is useful if the auto-generated file
///    name has changed (making the old file obsolete). See `is_magic_clean_fix_mode` for how to
///    activate this mode. Some things to note on the clean mode:
///    * The directory is cleaned only on the first registration of a "magic" file in the calling
///      crate.
///    * The directory is created if it does not exist.
///    * If you run clean mode on a specific test, you will delete all "magic" files of all tests of
///      the respective crate, regardless of whether or not the respective test was run. To avoid
///      this, never run clean mode on a single test; only on entire crates.
///    * If specific tests are run only when specific features are enabled, you should run the clean
///      mode with the same features enabled. Otherwise, the files will be deleted, but not
///      recreated.
#[macro_export]
macro_rules! register_magic_constants {
    () => {{
        // Both `canonicalize` and `function_name!` must be called in the macro context, to resolve
        // the caller relative path / function name correctly.
        let current_dir = std::fs::canonicalize(".").unwrap_or_else(|error| {
            panic!("Failed to get absolute path to current location: {error}.")
        });
        let function_name = $crate::function_name!();

        $crate::regression_test_utils::register_magic_constants_logic(&current_dir, function_name)
    }};
}
