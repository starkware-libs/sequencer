use std::fs;
use std::path::Path;

/// Recursively copies the contents of a directory from `src` to `dst`.
///
/// Creates the destination directory if it doesn't exist. If a file or directory
/// with the same name already exists in the destination, it will be overwritten.
/// If `src` and `dst` are the same path, this function returns early without doing anything.
///
/// # Panics
///
/// Panics if any I/O operation fails, including:
/// - Creating the destination directory
/// - Reading the source directory
/// - Copying files
/// - Accessing file metadata
pub(crate) fn copy_dir_contents(src: &Path, dst: &Path) {
    // Ensure destination exists.
    fs::create_dir_all(dst)
        .unwrap_or_else(|e| panic!("Failed to create directory {}: {}", dst.display(), e));

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
