use apollo_sierra_compilation_config::config::AllowedLibfuncsList;
use cairo_lang_starknet_classes::allowed_libfuncs::{
    BUILTIN_ALL_LIBFUNCS_LIST,
    BUILTIN_AUDITED_LIBFUNCS_LIST,
};

use super::libfunc_list_arg;
use crate::constants::BUNDLED_ALLOWED_LIBFUNCS_PATH;

#[test]
fn built_in_libfunc_lists_are_selected_by_name() {
    assert_eq!(
        libfunc_list_arg(AllowedLibfuncsList::Audited),
        ("--allowed-libfuncs-list-name", BUILTIN_AUDITED_LIBFUNCS_LIST.to_owned())
    );
    assert_eq!(
        libfunc_list_arg(AllowedLibfuncsList::All),
        ("--allowed-libfuncs-list-name", BUILTIN_ALL_LIBFUNCS_LIST.to_owned())
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
