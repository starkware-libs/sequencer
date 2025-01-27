# To be used together with a bazel WORKSPACE file that contains the following definitions.
load("@committer_crate//:defs.bzl", "all_crate_deps")
load("@rules_rust//rust:defs.bzl", "rust_binary")

rust_binary(
    name="committer_exe",
    srcs=glob(["src/**/*.rs"]),
    visibility=["//visibility:public"],
    deps=all_crate_deps(),
    proc_macro_deps=all_crate_deps(proc_macro=True),
    edition="2021",
)
