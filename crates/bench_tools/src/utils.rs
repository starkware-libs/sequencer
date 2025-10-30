use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Recursively copies the contents of a directory from `src` to `dst`.
///
/// Creates the destination directory if it doesn't exist. If a file or directory
/// with the same name already exists in the destination, it will be overwritten.
/// If `src` and `dst` are the same path, this function returns early without doing anything.
///
/// # Panics
/// Panics if the source or destination paths are files.
/// Panics if any I/O operation fails, including:
/// - Creating the destination directory
/// - Reading the source directory
/// - Copying files
/// - Accessing file metadata
pub(crate) fn copy_dir_contents(src: &Path, dst: &Path) {
    // Ensure destination exists.
    fs::create_dir_all(dst)
        .unwrap_or_else(|e| panic!("Failed to create directory {}: {}", dst.display(), e));

    // Verify that source and destination are directories.
    if !src.is_dir() {
        panic!("Source path is not a directory: {}", src.display());
    }
    if !dst.is_dir() {
        panic!("Destination path is not a directory: {}", dst.display());
    }

    // No-op if source and destination are the same.
    // Canonicalize paths to handle relative vs absolute paths and symlinks.
    let src_canonical = src
        .canonicalize()
        .unwrap_or_else(|e| panic!("Failed to canonicalize source path {}: {}", src.display(), e));
    let dst_canonical = dst.canonicalize().unwrap_or_else(|e| {
        panic!("Failed to canonicalize destination path {}: {}", dst.display(), e)
    });

    if src_canonical == dst_canonical {
        return;
    }

    let entries = fs::read_dir(src)
        .unwrap_or_else(|e| panic!("Failed to read directory {}: {}", src.display(), e));

    for entry in entries {
        let entry = entry.unwrap_or_else(|e| panic!("Failed to read directory entry: {}", e));
        let from = entry.path();
        let to = dst.join(entry.file_name());

        let file_type = entry
            .file_type()
            .unwrap_or_else(|e| panic!("Failed to get file type for {}: {}", from.display(), e));

        if file_type.is_dir() {
            copy_dir_contents(&from, &to);
        } else {
            fs::copy(&from, &to).unwrap_or_else(|e| {
                panic!("Failed to copy {} to {}: {}", from.display(), to.display(), e)
            });
        }
    }
}

/// Parses a flat Vec<String> of benchmark names and limits into a HashMap.
/// The input vector should contain pairs: [bench_name1, limit1, bench_name2, limit2, ...].
///
/// # Panics
/// Panics if any limit value cannot be parsed as f64.
pub fn parse_absolute_time_limits(args: Vec<String>) -> HashMap<String, f64> {
    let mut limits = HashMap::new();
    for chunk in args.chunks(2) {
        if chunk.len() == 2 {
            let bench_name = chunk[0].clone();
            let limit = chunk[1].parse::<f64>().unwrap_or_else(|_| {
                panic!("Invalid limit value for benchmark '{}': '{}'", bench_name, chunk[1])
            });
            limits.insert(bench_name, limit);
        }
    }
    limits
}
