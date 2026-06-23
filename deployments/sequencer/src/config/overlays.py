"""Shared overlay-path resolution for the deployment config loaders.

An overlay is given in dot notation that MUST start with the layout name
(e.g. `hybrid.mainnet.apollo-mainnet-0`) and maps to the directory
`configs/overlays/<layout>/mainnet/apollo-mainnet-0` under the base dir.

Both the YAML loader (`app._get_config_paths`) and the native jsonnet layer resolver
(`native.resolve_bucket_files`) walk the same overlay dirs in the same order, so the walk +
validation lives here once.
"""

from pathlib import Path


def overlay_dirs(base_dir: Path, layout: str, overlays: list[str]) -> list[Path]:
    """Resolve each dotted `-o` overlay to its directory under `configs/overlays/<layout>`.

    Validates that every overlay path starts with the layout name. Returns the dirs in the same
    order as `overlays` (the deep-merge / override order, last wins). Existence is NOT checked —
    callers decide what to do with a dir that holds no files.
    """
    overlays_root = base_dir / "configs" / "overlays" / layout
    dirs: list[Path] = []
    for overlay in overlays:
        segments = overlay.split(".")
        if not segments or segments[0] != layout:
            raise ValueError(
                f"Overlay path '{overlay}' must start with the layout name '{layout}'. "
                f"Example: '{layout}.mainnet.apollo-mainnet-0'"
            )
        overlay_dir = overlays_root
        for segment in segments[1:]:
            overlay_dir = overlay_dir / segment
        dirs.append(overlay_dir)
    return dirs
