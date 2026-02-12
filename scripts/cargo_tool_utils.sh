function install_toolchain_and_rustfmt_for_it() {
    # Redirect to stderr so output doesn't pollute command substitution captures.
    rustup toolchain install "$1" >&2
    rustup component add --toolchain "$1" rustfmt >&2
}

function verify_and_return_fmt_toolchain() {
    TOOLCHAIN=nightly-2024-04-29
    rustup toolchain list | grep -q "${TOOLCHAIN}" || install_toolchain_and_rustfmt_for_it "${TOOLCHAIN}"
    echo "${TOOLCHAIN}"
}
