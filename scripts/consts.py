from typing import Set

# Set of files which - if changed - should trigger tests for all packages.
ALL_TEST_TRIGGERS: Set[str] = {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml"}
