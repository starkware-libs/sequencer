use indoc::indoc;

pub(crate) const LOG2_CEIL: &str = indoc! {r#"from starkware.python.math_utils import log2_ceil
    ids.res = log2_ceil(ids.value)"#
};
