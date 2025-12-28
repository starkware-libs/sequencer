use std::path::Path;

use tempfile::TempDir;

use crate::compile_program::cairo_root_path;

const VIRTUAL_SUFFIX: &str = "__virtual";

/// Result of preparing a virtual Cairo root directory.
pub struct VirtualCairoRoot {
    /// The temporary directory containing the swapped files.
    pub temp_dir: TempDir,
    /// List of virtual files that were swapped (relative paths from cairo root).
    pub swapped_files: Vec<String>,
}

/// Prepares a temporary Cairo root directory with `__virtual` files swapped in.
///
/// This function:
/// 1. Creates a temporary directory
/// 2. Recursively copies all Cairo source files
/// 3. Finds all `*__virtual.cairo` files and renames them to `*.cairo` (overwriting originals)
///
/// Returns a `VirtualCairoRoot` containing the temp dir and list of swapped files.
pub fn prepare_virtual_cairo_root() -> std::io::Result<VirtualCairoRoot> {
    let source_root = cairo_root_path();
    let temp_dir = TempDir::new()?;

    // Copy all files from source to temp directory.
    copy_dir_recursive(&source_root, temp_dir.path())?;

    // Find and swap all __virtual files.
    let mut swapped_files = Vec::new();
    swap_virtual_files(temp_dir.path(), temp_dir.path(), &mut swapped_files)?;

    // Sort for deterministic output.
    swapped_files.sort();

    Ok(VirtualCairoRoot { temp_dir, swapped_files })
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
/// Appends swapped file paths (relative to root) to `swapped_files`.
fn swap_virtual_files(
    dir: &Path,
    root: &Path,
    swapped_files: &mut Vec<String>,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();

        if file_type.is_dir() {
            swap_virtual_files(&path, root, swapped_files)?;
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

                    // Record the swapped file (relative path).
                    let relative_path = new_path
                        .strip_prefix(root)
                        .map_err(std::io::Error::other)?;
                    swapped_files.push(relative_path.to_string_lossy().into_owned());

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
