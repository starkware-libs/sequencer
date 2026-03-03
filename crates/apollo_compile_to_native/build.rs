use std::path::Path;

use apollo_compilation_utils::build_utils::install_compiler_binary;

include!("src/constants.rs");

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // Rerun when workspace cairo-native dependency changes.
    println!("cargo:rerun-if-changed=../../Cargo.toml");

    set_run_time_out_dir_env_var();
    install_starknet_native_compile();
}

/// Install the `starknet-native-compile` binary from the Cairo Native crate and moves the binary
/// to the `target` directory. The `starknet-native-compile` binary is used to compile Sierra to
/// Native. The binary is executed as a subprocess whenever Sierra to Cairo compilation is required.
/// Installation is driven by the workspace Cargo.toml's cairo-native dependency (single source of
/// truth).
fn install_starknet_native_compile() {
    let binary_name = CAIRO_NATIVE_BINARY_NAME;
    let (required_version, cargo_install_args) = cargo_install_args_from_workspace();
    let args_refs: Vec<&str> = cargo_install_args.iter().map(String::as_str).collect();
    install_compiler_binary(binary_name, &required_version, &args_refs, &out_dir());
}

/// Reads the workspace root Cargo.toml and builds (required_version, cargo_install_args) from
/// the workspace.dependencies["cairo-native"] entry.
fn cargo_install_args_from_workspace() -> (String, Vec<String>) {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let workspace_cargo = Path::new(&manifest_dir).join("../../Cargo.toml");
    let contents = std::fs::read_to_string(&workspace_cargo).unwrap_or_else(|e| {
        panic!("Failed to read workspace Cargo.toml at {:?}: {}", workspace_cargo, e)
    });
    let root: toml::Table = contents.parse().expect("Failed to parse workspace Cargo.toml");
    let workspace = root
        .get("workspace")
        .and_then(|v| v.as_table())
        .expect("workspace Cargo.toml has no [workspace]");
    let deps = workspace
        .get("dependencies")
        .and_then(|v| v.as_table())
        .expect("[workspace] has no dependencies section");
    let cairo_native =
        deps.get("cairo-native").expect("workspace.dependencies has no cairo-native entry");

    match cairo_native {
        toml::Value::String(version) => {
            let version = version.trim_start_matches('=');
            let args = vec![binary_name_arg(), "--version".to_string(), version.to_string()];
            (version.to_string(), args)
        }
        toml::Value::Table(t) => {
            let version = t.get("version").and_then(|v| v.as_str());
            let git = t.get("git").and_then(|v| v.as_str());
            let branch = t.get("branch").and_then(|v| v.as_str());
            let tag = t.get("tag").and_then(|v| v.as_str());
            let rev = t.get("rev").and_then(|v| v.as_str());

            let required_version = version
                .map(|s| s.trim_start_matches('=').to_string())
                .or_else(|| tag.map(String::from))
                .or_else(|| rev.map(String::from))
                .or_else(|| branch.map(String::from))
                .expect("cairo-native must have version, or git with branch/tag/rev");

            let mut args = vec![binary_name_arg()];
            if let Some(url) = git {
                args.push("--git".to_string());
                args.push(url.to_string());
                if let Some(b) = branch {
                    args.push("--branch".to_string());
                    args.push(b.to_string());
                }
                if let Some(t) = tag {
                    args.push("--tag".to_string());
                    args.push(t.to_string());
                }
                if let Some(r) = rev {
                    args.push("--rev".to_string());
                    args.push(r.to_string());
                }
            } else if let Some(v) = version {
                args.push("--version".to_string());
                args.push(v.trim_start_matches('=').to_string());
            } else {
                panic!("cairo-native must specify version or git with branch/tag/rev");
            }
            (required_version, args)
        }
        _ => panic!("workspace.dependencies.cairo-native must be a string or a table"),
    }
}

fn binary_name_arg() -> String {
    CAIRO_NATIVE_BINARY_NAME.to_string()
}

// Sets the `RUNTIME_ACCESSIBLE_OUT_DIR` environment variable to the `OUT_DIR` value, which will be
// available only after the build is completed. Most importantly, it is available during runtime.
fn set_run_time_out_dir_env_var() {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR is not set");
    println!("cargo:rustc-env=RUNTIME_ACCESSIBLE_OUT_DIR={out_dir}");
}

// Returns the OUT_DIR. This function is only operable at build time.
fn out_dir() -> std::path::PathBuf {
    std::env::var("OUT_DIR")
        .expect("Failed to get the build time OUT_DIR environment variable")
        .into()
}
