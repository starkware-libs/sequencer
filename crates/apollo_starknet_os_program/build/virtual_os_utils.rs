use std::path::Path;

use tempfile::TempDir;

use crate::compile_program::cairo_root_path;

const VIRTUAL_SUFFIX: &str = "__virtual";

/// Prepares a temporary Cairo root directory with `__virtual` files swapped in.
///
/// This function:
/// 1. Creates a temporary directory
/// 2. Recursively copies all Cairo source files
/// 3. Finds all `*__virtual.cairo` files and renames them to `*.cairo` (overwriting originals)
///
/// Returns a `TempDir` that auto-deletes when dropped.
pub fn prepare_virtual_cairo_root() -> std::io::Result<TempDir> {
    let source_root = cairo_root_path();
    let temp_dir = TempDir::new()?;

    // Copy all files from source to temp directory.
    copy_dir_recursive(&source_root, temp_dir.path())?;

    // Find and swap all __virtual files.
    swap_virtual_files(temp_dir.path())?;

    Ok(temp_dir)
}

/// Recursively copies a directory and its contents.
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;

    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else if file_type.is_file() {
            std::fs::copy(&src_path, &dst_path)?;
        } else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                format!("Unexpected file type at {:?}", src_path),
            ));
        }
    }

    Ok(())
}

/// Recursively finds all `*__virtual.cairo` files and renames them to `*.cairo`.
/// Fails if the corresponding non-virtual file doesn't exist.
fn swap_virtual_files(dir: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();

        if file_type.is_dir() {
            swap_virtual_files(&path)?;
        } else if file_type.is_file() {
            if let Some(file_name) = path.file_stem().and_then(|s| s.to_str()) {
                if file_name.ends_with(VIRTUAL_SUFFIX) {
                    // Compute the non-virtual file name: foo__virtual.cairo -> foo.cairo.
                    let base_name = file_name.strip_suffix(VIRTUAL_SUFFIX).unwrap();
                    let new_name = format!("{base_name}.cairo");
                    let new_path = path.with_file_name(new_name);

                    // Fail if the non-virtual file doesn't exist.
                    if !new_path.exists() {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::NotFound,
                            format!(
                                "Virtual file {:?} has no corresponding non-virtual file {:?}",
                                path, new_path
                            ),
                        ));
                    }

                    // Remove the original file, then rename virtual -> original.
                    std::fs::remove_file(&new_path)?;
                    std::fs::rename(&path, &new_path)?;
                }
            }
        } else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                format!("Unexpected file type at {:?}", path),
            ));
        }
    }

    Ok(())
}
