#!/usr/bin/env python3
"""Unit tests for the publish order in publish_crates (no crates.io access needed)."""

from typing import List, Optional

import pytest
from publish_crates import (
    get_package_and_dependencies_in_order,
    get_publishable_crates,
    get_unpublishable_crates,
    get_workspace_packages,
)

VERSIONED = "^0.0.0"
PATH_ONLY = "*"


def _dependency(name: str, req: str, kind: Optional[str] = None, optional: bool = False) -> dict:
    return {"name": name, "req": req, "kind": kind, "optional": optional}


def _package(
    name: str,
    dependencies: List[dict],
    publishable: bool = True,
    registries: Optional[List[str]] = None,
) -> dict:
    publish = registries if registries is not None else (None if publishable else [])
    return {"name": name, "dependencies": dependencies, "publish": publish}


def test_publishable_crates_are_those_crates_io_accepts():
    packages = [
        _package("unrestricted", []),
        _package("publish_false", [], publishable=False),
        _package("crates_io_only", [], registries=["crates-io"]),
        _package("private_registry_only", [], registries=["internal-registry"]),
    ]
    assert get_publishable_crates(packages) == {"unrestricted", "crates_io_only"}


def test_unpublishable_crates_are_reported_in_order():
    packages = [
        _package("app", []),
        _package("internal_b", [], publishable=False),
        _package("internal_a", [], publishable=False),
    ]
    assert get_unpublishable_crates(["internal_b", "app", "internal_a"], packages) == [
        "internal_b",
        "internal_a",
    ]


def test_dependencies_come_before_dependents_and_crate_is_last():
    packages = [
        _package("app", [_dependency("mid", VERSIONED)]),
        _package("mid", [_dependency("base", VERSIONED)]),
        _package("base", []),
    ]
    assert get_package_and_dependencies_in_order("app", packages) == ["base", "mid", "app"]


def test_shared_dependency_appears_once_before_both_dependents():
    packages = [
        _package("app", [_dependency("left", VERSIONED), _dependency("right", VERSIONED)]),
        _package("left", [_dependency("base", VERSIONED)]),
        _package("right", [_dependency("base", VERSIONED)]),
        _package("base", []),
    ]
    assert get_package_and_dependencies_in_order("app", packages) == [
        "base",
        "left",
        "right",
        "app",
    ]


def test_optional_normal_dependency_is_published_first():
    packages = [
        _package("app", [_dependency("helper", VERSIONED, optional=True)]),
        _package("helper", []),
    ]
    assert get_package_and_dependencies_in_order("app", packages) == ["helper", "app"]


def test_build_dependency_is_published_first():
    packages = [
        _package("app", [_dependency("codegen", VERSIONED, kind="build")]),
        _package("codegen", []),
    ]
    assert get_package_and_dependencies_in_order("app", packages) == ["codegen", "app"]


def test_versioned_dev_dependency_is_published_first():
    packages = [
        _package("app", [_dependency("fixtures", VERSIONED, kind="dev")]),
        _package("fixtures", []),
    ]
    assert get_package_and_dependencies_in_order("app", packages) == ["fixtures", "app"]


def test_path_only_dev_dependency_is_left_out():
    packages = [
        _package("app", [_dependency("test_helper", PATH_ONLY, kind="dev")]),
        _package("test_helper", [], publishable=False),
    ]
    assert get_package_and_dependencies_in_order("app", packages) == ["app"]


def test_path_only_normal_dependency_is_kept_so_the_publish_check_rejects_it():
    # Cargo rejects publishing a normal dependency without a version, so it stays in the order
    # and the `publish = false` check names it.
    packages = [
        _package("app", [_dependency("internal", PATH_ONLY)]),
        _package("internal", [], publishable=False),
    ]
    order = get_package_and_dependencies_in_order("app", packages)
    assert order == ["internal", "app"]
    assert get_unpublishable_crates(order, packages) == ["internal"]


def test_non_member_dependency_is_left_out():
    packages = [_package("app", [_dependency("serde", "^1.0")])]
    assert get_package_and_dependencies_in_order("app", packages) == ["app"]


def test_dependency_cycle_is_refused():
    packages = [
        _package("app", [_dependency("fixtures", VERSIONED, kind="dev")]),
        _package("fixtures", [_dependency("app", VERSIONED)]),
    ]
    with pytest.raises(ValueError, match="app -> fixtures -> app"):
        get_package_and_dependencies_in_order("app", packages)


def test_unknown_crate_is_refused():
    with pytest.raises(ValueError, match="not a workspace member"):
        get_package_and_dependencies_in_order("missing", [_package("app", [])])


@pytest.fixture(scope="module")
def workspace_packages() -> List[dict]:
    return get_workspace_packages()


def _assert_published_before(order: List[str], dependency: str, dependent: str):
    assert dependency in order, f"{dependency} is missing from {order}"
    assert order.index(dependency) < order.index(dependent), order


def test_blockifier_order_includes_its_optional_dependencies(workspace_packages):
    order = get_package_and_dependencies_in_order("blockifier", workspace_packages)
    for dependency in [
        "apollo_compilation_utils",
        "apollo_compile_to_native",
        "blockifier_test_utils",
        "starknet_api",
    ]:
        _assert_published_before(order, dependency, "blockifier")


def test_os_program_order_includes_its_versioned_dev_dependencies(workspace_packages):
    order = get_package_and_dependencies_in_order("apollo_starknet_os_program", workspace_packages)
    for dependency in ["blockifier", "starknet_api", "apollo_infra_utils"]:
        _assert_published_before(order, dependency, "apollo_starknet_os_program")


def test_every_publishable_crate_orders_only_publishable_crates(workspace_packages):
    publishable_crates = get_publishable_crates(workspace_packages)
    # Guards the loop below against passing vacuously on an empty or all-inclusive set.
    assert "blockifier" in publishable_crates
    assert "apollo_batcher" not in publishable_crates
    for crate in sorted(publishable_crates):
        order = get_package_and_dependencies_in_order(crate, workspace_packages)
        unpublishable = get_unpublishable_crates(order, workspace_packages)
        assert not unpublishable, f"{crate} would publish {unpublishable}"
