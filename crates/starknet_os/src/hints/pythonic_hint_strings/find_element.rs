use indoc::indoc;

// Skip formatting of the following hint - rustfmt appears to decrease the indentation of the line
// after "assert n_elms <= __find_element_max_size, \", which breaks hint identification.
#[rustfmt::skip]
pub(crate) const SEARCH_SORTED_OPTIMISTIC: &str = indoc! {r#"
    array_ptr = ids.array_ptr
    elm_size = ids.elm_size
    assert isinstance(elm_size, int) and elm_size > 0, \
        f'Invalid value for elm_size. Got: {elm_size}.'

    n_elms = ids.n_elms
    assert isinstance(n_elms, int) and n_elms >= 0, \
        f'Invalid value for n_elms. Got: {n_elms}.'
    if '__find_element_max_size' in globals():
        assert n_elms <= __find_element_max_size, \
            f'find_element() can only be used with n_elms<={__find_element_max_size}. ' \
            f'Got: n_elms={n_elms}.'

    for i in range(n_elms):
        if memory[array_ptr + elm_size * i] >= ids.key:
            ids.index = i
            ids.exists = 1 if memory[array_ptr + elm_size * i] == ids.key else 0
            break
    else:
        ids.index = n_elms
        ids.exists = 0"#};
