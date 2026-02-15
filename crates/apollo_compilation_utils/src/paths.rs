use std::ffi::OsStr;
use std::path::{Path, PathBuf};

fn profile_dir_from_executable(executable_path: &Path) -> PathBuf {
    let executable_dir =
        executable_path.parent().expect("Current executable path has no parent directory");
    if executable_dir.file_name() == Some(OsStr::new("deps")) {
        executable_dir
            .parent()
            .expect("Failed to navigate from deps to profile directory")
            .to_path_buf()
    } else {
        executable_dir.to_path_buf()
    }
}

fn profile_dir() -> PathBuf {
    let executable_path = std::env::current_exe().expect("Failed to resolve current executable.");
    profile_dir_from_executable(&executable_path)
}

pub fn shared_folder_dir() -> PathBuf {
    profile_dir().join("shared_executables")
}

pub fn binary_path(binary_name: &str) -> PathBuf {
    shared_folder_dir().join(binary_name)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::profile_dir_from_executable;

    #[test]
    fn resolve_profile_dir_for_test_binary() {
        let executable_path = PathBuf::from("/tmp/target/debug/deps/test_binary");
        let profile_dir = profile_dir_from_executable(&executable_path);

        assert_eq!(profile_dir, PathBuf::from("/tmp/target/debug"));
    }

    #[test]
    fn resolve_profile_dir_for_regular_binary() {
        let executable_path = PathBuf::from("/tmp/target/release/install_binary");
        let profile_dir = profile_dir_from_executable(&executable_path);

        assert_eq!(profile_dir, PathBuf::from("/tmp/target/release"));
    }
}
