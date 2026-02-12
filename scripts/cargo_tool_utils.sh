function install_toolchain_and_rustfmt_for_it() {
    rustup toolchain install "$1"
    rustup component add --toolchain "$1" rustfmt
}

function verify_and_return_fmt_toolchain() {
    TOOLCHAIN=nightly-2024-04-29
    rustup toolchain list | grep -q "${TOOLCHAIN}" || install_toolchain_and_rustfmt_for_it "${TOOLCHAIN}"
    echo "${TOOLCHAIN}"
}
