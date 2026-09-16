use apollo_sierra_compilation_config::config::AllowedLibfuncsList;

use super::libfunc_list_arg;
use crate::constants::BUNDLED_ALLOWED_LIBFUNCS_PATH;

#[test]
fn built_in_libfunc_lists_are_selected_by_name() {
    assert_eq!(
        libfunc_list_arg(AllowedLibfuncsList::Audited),
        ("--allowed-libfuncs-list-name", "audited".to_owned())
    );
    assert_eq!(
        libfunc_list_arg(AllowedLibfuncsList::All),
        ("--allowed-libfuncs-list-name", "all".to_owned())
    );
}

#[test]
fn bundled_libfunc_list_is_selected_by_the_shipped_file() {
    let (flag, path) = libfunc_list_arg(AllowedLibfuncsList::Bundled);

    assert_eq!(flag, "--allowed-libfuncs-list-file");
    assert!(
        path.ends_with(BUNDLED_ALLOWED_LIBFUNCS_PATH),
        "expected a path ending in {BUNDLED_ALLOWED_LIBFUNCS_PATH}, got {path}"
    );
}
