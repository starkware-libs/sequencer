use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

use serde::Serialize;
use serde_json::Value;

/// Utilities for regression tests with "magic" values: values that are tested against computed
/// values, and are stored in JSON files.
///
/// For example, the old way of doing things looks something like this:
/// ```rust
/// #[test]
/// fn test_something() {
///     let computation_result = 3 + 4;
///     assert_eq!(computation_result, 7);
/// }
/// ```
///
/// To use the new method, you need to add the `register_magic_constants!` macro to the test, and
/// assert using the `MagicConstants` object:
/// ```rust
/// #[test]
/// fn test_something() {
///     let mut magic = register_magic_constants!("");
///     let computation_result = 3 + 4;
///     magic.assert_eq("MY_VALUE", computation_result);
/// }
/// ```
///
/// Then, generate the JSON file with the default values by running:
/// ```bash
/// MAGIC_FIX=1 cargo test -p <MY_CRATE> test_something
/// ```
///
/// This will create a JSON file in the `magic_constants` directory of the calling crate, with the
/// dict `{ "MY_VALUE": 7 }`.
///
/// Note that the registration of the "magic" constants must generate unique filenames, which is
/// non-trivial in parametrized tests; the argument to the `register_magic_constants!` macro must
/// be unique for each test case. For example:
/// ```rust
/// #[rstest]
/// fn test_something(#[values(1, 2)] value: u32) {
///     let mut magic = register_magic_constants!(format!("value_{value}"));
///     let computation_result = value + 6;
///     magic.assert_eq("MY_VALUE", computation_result);
/// }
/// ```
///
/// This will generate two separate files in the `magic_constants` directory, one per test case.
/// The expected values in each test case may be identical, or different, but the filenames must be
/// unique.

/// Global registry for magic constants files. Used to keep track of the "magic number" files that
/// are generated / used by regression tests.
#[derive(Default)]
pub struct MagicConstantsRegistry(pub Mutex<HashSet<String>>);

pub static MAGIC_CONSTANTS_REGISTRY: LazyLock<MagicConstantsRegistry> =
    LazyLock::new(MagicConstantsRegistry::default);

/// Check if we are in "clean" mode. In this mode, we delete all files in the magic constants
/// directory before creating a new one. This is used to keep the regression files "clean" (in case
/// a file / test function was renamed, we don't want to keep dangling JSON artifacts).
pub fn is_magic_clean_fix_mode() -> bool {
    std::env::var("MAGIC_CLEAN_FIX").is_ok()
}

/// Check if we are in "fix" mode. In this mode, we create a new file with the default values.
pub fn is_magic_fix_mode() -> bool {
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
    pub fn new(path: String, values: BTreeMap<String, Value>) -> Self {
        Self { path, values }
    }

    /// Main function to assert the equality of a value with the one in the file.
    /// If you have a test that uses a magic constant, you should use this function to assert the
    /// equality of the value.
    /// For example, `assert_eq!(computed_value, 7)` should be replaced with
    /// `magic.assert_eq("MY_VALUE", computed_value)`.
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

/// In fix mode, automatically dump the values to the file on drop (when test ends).
impl Drop for MagicConstants {
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

/// If we are in CLEAN mode, and this is the first registration of a file in the current
/// directory, we need to delete all files in the directory (and possibly create the
/// directory) to keep the regression files "clean" (in case a file / test function was
/// renamed, we don't want to keep dangling JSON artifacts).
pub fn clean_if_first_registration(current_dir: PathBuf, magic_subdir: &PathBuf) {
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

/// Given the directory, the function name and the unique string provided in the macro, registers
/// the JSON file (panics if already registered) and returns the path to the file.
pub fn register_and_return_path(
    directory: &PathBuf,
    function_name: &str,
    unique_string: String,
) -> String {
    let bad_chars = regex::Regex::new(r"[:()\[\]]").unwrap();
    let magic_filename =
        bad_chars.replace_all(&format!("{function_name}_{unique_string}.json"), "_").to_string();
    let path = directory.join(magic_filename).to_str().unwrap().to_string();
    if !MAGIC_CONSTANTS_REGISTRY.0.lock().unwrap().insert(path.clone()) {
        panic!("Magic constants file already registered: {path}");
    }
    path
}

pub fn load_magic_constants(absolute_path: &PathBuf) -> MagicConstants {
    let file = std::fs::File::open(absolute_path).unwrap_or_else(|error| {
        panic!("Failed to open magic constants file at {absolute_path:?}: {error}.")
    });
    let reader = std::io::BufReader::new(file);
    let json: serde_json::Value = serde_json::from_reader(reader).unwrap();
    let values =
        std::collections::BTreeMap::from_iter(json.as_object().unwrap().clone().into_iter());
    MagicConstants::new(absolute_path.to_str().unwrap().to_string(), values)
}

/// Main logic of this module. Used to register and initialize the magic constants for a specific
/// test.
/// Each registration corresponds to a unique JSON file in the `magic_constants` directory of the
/// calling crate.
/// If the same file is registered twice, it will panic.
///
/// The macro behaves differently depending on the mode:
/// 1. If vanilla `cargo test` is run (no fix / clean modes), it will load the values from the file.
///    If the file does not exist, it will panic.
/// 2. If we are in fix mode, but not clean mode, a new file will be created (with an empty object).
///    Note that this will not delete any existing files, unless the name is identical.
/// 3. If we are in clean mode, all files in the `magic_constants` directory of the calling crate
///    will be deleted before new files are registered. This is useful if the auto-generated file
///    name has changed (making the old file obsolete). a. The directory is cleaned only on the
///    first registration of a "magic" file in the calling crate. b. The directory is created if it
///    does not exist. c. Note that if you run clean mode on a specific test, you will delete all
///    "magic" files of all tests of this crate, regardless of whether or not the respective test
///    was run. To avoid this, never run clean mode on a single test; only on entire crates.
#[macro_export]
macro_rules! register_magic_constants {
    ($unique_name:expr) => {{
        let directory = std::path::PathBuf::from("magic_constants");
        let current_dir = std::fs::canonicalize(".").unwrap_or_else(|error| {
            panic!("Failed to get absolute path to current location: {error}.")
        });

        $crate::regression_test_utils::clean_if_first_registration(current_dir, &directory);

        let directory = std::fs::canonicalize(&directory).unwrap_or_else(|error| {
            panic!("Failed to get absolute path for magic constants directory: {error}.")
        });
        let function_name = $crate::function_name!();

        // Register the path.
        let path = $crate::regression_test_utils::register_and_return_path(
            &directory,
            function_name,
            $unique_name.to_string(),
        );

        // If the file doesn't exist, create it with an empty object.
        // This should be done in the macro context, and not in a function call, as the path to the
        // file is relative to the current directory.
        if !std::path::Path::new(&path).exists() {
            std::fs::File::create(&path).unwrap_or_else(|error| {
                panic!("Failed to create magic constants file at {path}: {error}.")
            });
            std::fs::write(&path, "{}")
                .unwrap_or_else(|error| panic!("Failed to write empty dict to {path}: {error}."));
        }

        let absolute_path = std::fs::canonicalize(&path).unwrap_or_else(|error| {
            panic!("Failed to get absolute path for magic constants file at {path}: {error}.")
        });

        $crate::regression_test_utils::load_magic_constants(&absolute_path)
    }};
}
