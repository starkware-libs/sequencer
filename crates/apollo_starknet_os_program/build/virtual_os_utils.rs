use std::collections::HashSet;
use std::io;
use std::path::PathBuf;

use tempfile::TempDir;

use crate::compile_program::cairo_root_path;
use crate::dump_source::get_cairo_file_paths;

const VIRTUAL_SUFFIX: &str = "__virtual";

/// Prepares a temporary Cairo root directory with `__virtual` files swapped in.
///
/// Returns a `TempDir` that auto-deletes when dropped.
pub fn prepare_virtual_cairo_root() -> io::Result<TempDir> {
    let source_root = cairo_root_path();
    let temp_dir = TempDir::new()?;

    let paths = get_cairo_file_paths(&source_root);
    let replace_by_virtual: HashSet<_> =
        paths.iter().filter(|p| is_virtual_file(p)).map(|p| to_non_virtual_path(p)).collect();

    // Filter out non-virtual files that have a virtual counterpart.
    let paths_to_copy: Vec<_> = paths.iter().filter(|p| !replace_by_virtual.contains(*p)).collect();

    // Each virtual file should replace exactly one non-virtual file.
    assert_eq!(paths_to_copy.len(), paths.len() - replace_by_virtual.len());

    for src_path in paths_to_copy {
        let relative = to_non_virtual_path(src_path).strip_prefix(&source_root).unwrap().to_owned();
        let dst_path = temp_dir.path().join(relative);
        std::fs::create_dir_all(dst_path.parent().unwrap())?;
        std::fs::copy(src_path, dst_path)?;
    }

    Ok(temp_dir)
}

fn is_virtual_file(path: &PathBuf) -> bool {
    path.file_stem().and_then(|s| s.to_str()).is_some_and(|s| s.ends_with(VIRTUAL_SUFFIX))
}

fn to_non_virtual_path(path: &PathBuf) -> PathBuf {
    let stem = path.file_stem().unwrap().to_str().unwrap();
    match stem.strip_suffix(VIRTUAL_SUFFIX) {
        Some(base) => path.with_file_name(format!("{base}.cairo")),
        None => path.clone(),
    }
}
