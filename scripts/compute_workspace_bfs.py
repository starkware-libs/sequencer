#!/home/aviv/Workspace/sequencer/sequencer_venv/bin/python

"""
Compute BFS layers of internal workspace dependencies for a given crate.

What it does:
- Reads the root Cargo.toml [workspace.dependencies] to find internal crates (path deps).
- For the target crate, traverses internal deps from [dependencies] and [build-dependencies]
  (dev-dependencies are excluded by default unless --include-dev is used).
- Prints BFS layers (L0 = the crate itself) or a reverse-BFS publish order.

Examples:
  ./scripts/compute_workspace_bfs.py --crate blockifier
  ./scripts/compute_workspace_bfs.py --crate blockifier --publish-order

Implementation notes:
- Run from the sequencer venv. Uses tomli for TOML parsing.
"""

from __future__ import annotations

import argparse
from collections import deque
from pathlib import Path
from typing import Dict, List, Set

# TOML parser: use tomli (works consistently across Python versions).
import tomli as _toml  # type: ignore


def parse_workspace_internal_crates(root: Path) -> Dict[str, Path]:
    """Map crate_name -> crate_dir for internal workspace path dependencies."""
    cargo_toml = root / "Cargo.toml"
    data = _toml.loads(cargo_toml.read_text(encoding="utf-8"))
    deps = data.get("workspace", {}).get("dependencies", {}) or {}
    internal: Dict[str, Path] = {}
    for name, spec in deps.items():
        if isinstance(spec, dict) and "path" in spec:
            internal[name] = (root / spec["path"]).resolve()
    return internal


def parse_crate_deps(crate_dir: Path, include_dev: bool, internal_names: Set[str]) -> Set[str]:
    """Internal deps from [dependencies] and [build-dependencies]; optionally dev-deps."""
    cargo_toml = crate_dir / "Cargo.toml"
    # Read the Cargo.toml file and parse it into a dictionary.
    data = _toml.loads(cargo_toml.read_text(encoding="utf-8"))

    def keys(section: str) -> Set[str]:
        table = data.get(section, {}) or {}
        return {name for name in table.keys() if name in internal_names}

    # Get the dependencies from the [dependencies] and [build-dependencies] sections.
    deps = keys("dependencies") | keys("build-dependencies")
    # If dev-dependencies are included, add them to the dependencies.
    if include_dev:
        deps |= keys("dev-dependencies")
    return deps


def bfs_layers(start: str, internal_map: Dict[str, Path], include_dev: bool) -> List[List[str]]:
    if start not in internal_map:
        raise SystemExit(f"crate '{start}' is not an internal workspace path dependency.")

    internal_names = set(internal_map.keys())
    visited: Set[str] = {start}
    layers: List[List[str]] = [[start]]

    q = deque([start])
    while q:
        # Build the next layer from the entire current crates layer in one step.
        current_crates_layer = list(q)
        q.clear()
        next_nodes: Set[str] = set()
        for crate in current_crates_layer:
            # Add unseen internal dependencies of each crate to the next layer.
            deps = parse_crate_deps(internal_map[crate], include_dev=include_dev, internal_names=internal_names)
            for dep in deps:
                if dep not in visited:
                    visited.add(dep)
                    next_nodes.add(dep)
        if next_nodes:
            # Sort for deterministic layer order and enqueue for the next round.
            layer_list = sorted(next_nodes)
            layers.append(layer_list)
            for n in layer_list:
                q.append(n)
    return layers


def publish_order_from_layers(layers: List[List[str]]) -> List[str]:
    """Reverse BFS by layers to get a safe publish order. Within a layer, publish in alphabetical order."""
    order: List[str] = []
    for layer in reversed(layers):
        order.extend(layer)
    return order


def main() -> None:
    parser = argparse.ArgumentParser(
        description="BFS of internal workspace deps for a crate (excluding dev-deps by default)."
    )
    parser.add_argument("--crate", required=True, help="Crate name (e.g., blockifier)")
    parser.add_argument("--root", default=str(Path(__file__).resolve().parents[1]), help="Repo root (default: project root)")
    parser.add_argument("--include-dev", action="store_true", help="Include dev-dependencies in traversal")
    parser.add_argument("--publish-order", action="store_true", help="Output publish order (reverse BFS) instead of BFS layers")
    args = parser.parse_args()

    # Normalize the provided repo root to an absolute, symlink-resolved Path.
    root = Path(args.root).resolve()
    # Build a mapping from crate name to its absolute directory for internal path deps.
    internal_map = parse_workspace_internal_crates(root)

    # Compute BFS layers of internal dependencies starting from the target crate.
    layers = bfs_layers(args.crate, internal_map, include_dev=args.include_dev)

    if args.publish_order:
        # Flatten layers in reverse to obtain a publish order (deepest first).
        order = publish_order_from_layers(layers)
        for name in order:
            print(name)
        return

    # Output BFS layers
    for depth, names in enumerate(layers):
        print(f"L{depth}: {', '.join(names)}")


if __name__ == "__main__":
    main()


