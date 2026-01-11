#!/usr/bin/env python3
"""
Dependency version report generator.

Generates a report showing for each dependency:
- TOML version (from workspace Cargo.toml)
- Locked version (from Cargo.lock via cargo metadata)
- Compatible version (latest semver-compatible via cargo update --dry-run)
- Latest version (from crates.io API)

Usage:
    # Default: run cargo commands, use crates.io cache + fetch missing
    python scripts/dependency_report.py

    # Cache only: use all cached data, no commands or network calls
    python scripts/dependency_report.py --mode cache-only

    # Refresh: ignore all cache, fetch everything fresh
    python scripts/dependency_report.py --mode refresh

    # Output as CSV for further processing
    python scripts/dependency_report.py --output csv > deps.csv

    # Output as JSON (includes summary)
    python scripts/dependency_report.py --output json > deps.json

    # Write output directly to a file
    python scripts/dependency_report.py --output csv -o deps.csv

Options:
    --mode MODE         Fetch mode: 'default', 'cache-only', 'refresh'
                        default:    Run cargo commands, use crates.io cache + fetch missing
                        cache-only: Use cached data only, no commands or network
                        refresh:    Ignore cache, fetch everything fresh
    --output FORMAT     Output format: 'table', 'json', 'csv' (default: table)
    --output-file FILE  Write output to FILE instead of stdout
    --cache-dir DIR     Directory to cache intermediate files (default: /tmp/dependency_report)
    --limit N           Limit crates.io API calls to N (for testing)

Exit codes:
    0 - All dependencies are up to date (within semver constraints)
    1 - Some dependencies have semver-compatible updates available

Data sources:
    1. cargo update --dry-run --verbose  -> Compatible updates
    2. cargo metadata                    -> Locked versions
    3. Cargo.toml [workspace.dependencies] -> TOML versions
    4. crates.io API                     -> Latest versions
"""

from __future__ import annotations

import argparse
import csv
import io
import itertools
import json
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Dict, List, Optional

try:
    import tomllib  # Python 3.11+
except ImportError:
    import tomli as tomllib  # Fallback for older Python


def progress_spinner(stop_event, message: str):
    """Display a spinning progress indicator."""
    spinner = itertools.cycle(["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
    while not stop_event.is_set():
        sys.stderr.write(f"\r{next(spinner)} {message}")
        sys.stderr.flush()
        stop_event.wait(0.1)
    sys.stderr.write(f"\r✓ {message}\n")
    sys.stderr.flush()


def run_command_with_progress(cmd: List[str], message: str) -> subprocess.CompletedProcess:
    """Run a command with a progress spinner."""
    stop_event = threading.Event()
    spinner_thread = threading.Thread(target=progress_spinner, args=(stop_event, message))
    spinner_thread.start()

    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            check=False,
        )
    finally:
        stop_event.set()
        spinner_thread.join()

    return result


def fetch_cargo_update_dry_run(cache_dir: Path) -> Optional[Path]:
    """
    Run 'cargo update --dry-run --verbose' and save output.

    Returns path to the output file, or None if command failed.
    """
    output_file = cache_dir / "cargo_update_dry_run.txt"

    result = run_command_with_progress(
        ["cargo", "update", "--dry-run", "--verbose"],
        "Running cargo update --dry-run...",
    )

    if result.returncode != 0:
        print(f"Warning: cargo update --dry-run failed: {result.stderr}", file=sys.stderr)
        return None

    # cargo update writes to stderr
    output = result.stdout + result.stderr

    output_file.write_text(output)

    return output_file


def fetch_cargo_metadata(cache_dir: Path) -> Path:
    """
    Run 'cargo metadata' and save output.

    Returns path to the output file.
    """
    output_file = cache_dir / "cargo_metadata.json"

    result = run_command_with_progress(
        ["cargo", "metadata", "--format-version", "1"], "Running cargo metadata..."
    )

    if result.returncode != 0:
        print(f"Error running cargo metadata: {result.stderr}", file=sys.stderr)
        sys.exit(1)

    output_file.write_text(result.stdout)

    return output_file


def parse_cargo_update_output(update_file: Path) -> Dict[str, Dict]:
    """
    Parse cargo update --dry-run output.

    Returns dict mapping crate name to update info:
    {
        "crate_name": {
            "current": "1.0.0",
            "compatible": "1.2.0",  # or None if unchanged
            "latest_available": "2.0.0",  # if mentioned in "(available: X)"
            "status": "updating" | "unchanged"
        }
    }
    """
    content = update_file.read_text()
    updates = {}

    for line in content.splitlines():
        line = line.strip()

        # Parse: "Updating crate vX.X.X -> vY.Y.Y" or "Updating crate vX.X.X -> vY.Y.Y (available: vZ.Z.Z)"
        if line.startswith("Updating "):
            parts = line.split()
            if len(parts) >= 4 and "->" in parts:
                crate_name = parts[1]
                current = parts[2].lstrip("v")
                arrow_idx = parts.index("->")
                compatible = parts[arrow_idx + 1].lstrip("v")

                # Check for "(available: vZ.Z.Z)"
                latest_available = None
                if "(available:" in line:
                    avail_match = line.split("(available:")[-1].rstrip(")")
                    latest_available = avail_match.strip().lstrip("v")

                updates[crate_name] = {
                    "current": current,
                    "compatible": compatible,
                    "latest_available": latest_available,
                    "status": "updating",
                }

        # Parse: "Unchanged crate vX.X.X (available: vY.Y.Y)"
        elif line.startswith("Unchanged "):
            parts = line.split()
            if len(parts) >= 3:
                crate_name = parts[1]
                current = parts[2].lstrip("v")

                latest_available = None
                if "(available:" in line:
                    avail_match = line.split("(available:")[-1].rstrip(")")
                    latest_available = avail_match.strip().lstrip("v")

                updates[crate_name] = {
                    "current": current,
                    "compatible": None,  # Already at latest compatible
                    "latest_available": latest_available,
                    "status": "unchanged",
                }

    return updates


def parse_cargo_metadata(metadata_file: Path) -> Dict[str, Dict]:
    """
    Parse cargo metadata JSON output.

    Returns dict mapping crate name to package info:
    {
        "crate_name": {
            "version": "1.0.0",
            "source": "registry" | "path" | "git",
            "is_workspace_member": bool,
        }
    }
    """
    content = json.loads(metadata_file.read_text())

    workspace_members = set(content.get("workspace_members", []))
    packages = {}

    for pkg in content.get("packages", []):
        name = pkg["name"]
        version = pkg["version"]
        source = pkg.get("source")

        # Determine source type
        if source is None:
            source_type = "path"
        elif source.startswith("registry"):
            source_type = "registry"
        elif source.startswith("git"):
            source_type = "git"
        else:
            source_type = "unknown"

        # Check if workspace member
        pkg_id = pkg.get("id", "")
        is_workspace = (
            any(pkg_id.startswith(m.split("#")[0]) for m in workspace_members) or source is None
        )

        # Handle multiple versions of same crate
        if name not in packages:
            packages[name] = []

        packages[name].append(
            {
                "version": version,
                "source": source_type,
                "is_workspace_member": is_workspace,
            }
        )

    return packages


def load_crates_io_cache(cache_dir: Path) -> Dict[str, Optional[str]]:
    """
    Load crates.io versions from cache file.

    Returns dict mapping crate name to latest version (or empty dict if no cache).
    """
    cache_file = cache_dir / "crates_io_versions.json"

    if cache_file.exists():
        try:
            cached = json.loads(cache_file.read_text())
            print(f"  ✓ Loaded {len(cached)} cached crates.io versions", file=sys.stderr)
            return cached
        except json.JSONDecodeError:
            print("  ⚠ Cache file corrupted, returning empty", file=sys.stderr)
            return {}
    else:
        print("  ⚠ No crates.io cache found", file=sys.stderr)
        return {}


def fetch_crates_io_versions(
    crate_names: List[str],
    cache_dir: Path,
    use_cache: bool = True,
    rate_limit_delay: float = 0.1,
) -> Dict[str, Optional[str]]:
    """
    Fetch latest versions from crates.io API.

    Returns dict mapping crate name to latest version (or None if not found).
    Caches results to avoid repeated API calls.
    """
    cache_file = cache_dir / "crates_io_versions.json"

    # Load cache if exists
    cached = {}
    if use_cache and cache_file.exists():
        try:
            cached = json.loads(cache_file.read_text())
            print(f"Loaded {len(cached)} cached crates.io versions", file=sys.stderr)
        except json.JSONDecodeError:
            cached = {}

    results = dict(cached)
    to_fetch = [name for name in crate_names if name not in cached]

    if not to_fetch:
        print("All crates.io versions cached, skipping API calls", file=sys.stderr)
        return results

    total = len(to_fetch)

    for i, name in enumerate(to_fetch):
        # Progress bar
        pct = (i + 1) * 100 // total
        bar_len = 30
        filled = bar_len * (i + 1) // total
        bar = "█" * filled + "░" * (bar_len - filled)
        sys.stderr.write(f"\r  [{bar}] {i + 1}/{total} ({pct}%) - {name[:30]:<30}")
        sys.stderr.flush()

        try:
            url = f"https://crates.io/api/v1/crates/{name}"
            req = urllib.request.Request(
                url, headers={"User-Agent": "dependency-report-script/1.0"}
            )
            with urllib.request.urlopen(req, timeout=10) as response:
                data = json.loads(response.read().decode())
                max_version = data.get("crate", {}).get("max_version")
                results[name] = max_version
        except urllib.error.HTTPError as e:
            if e.code == 404:
                results[name] = None  # Crate not found (probably a path dep)
            else:
                print(f"  Warning: HTTP {e.code} for {name}", file=sys.stderr)
                results[name] = None
        except Exception as e:
            print(f"  Warning: Error fetching {name}: {e}", file=sys.stderr)
            results[name] = None

        # Rate limiting
        time.sleep(rate_limit_delay)

    # Clear progress line and save cache
    sys.stderr.write("\r" + " " * 80 + "\r")
    sys.stderr.flush()
    cache_file.write_text(json.dumps(results, indent=2))
    print(f"✓ Fetched {len(results)} crates.io versions", file=sys.stderr)

    return results


def clear_crates_io_cache(cache_dir: Path) -> None:
    """Delete the crates.io cache file."""
    cache_file = cache_dir / "crates_io_versions.json"
    if cache_file.exists():
        cache_file.unlink()
        print("  ✓ Cleared crates.io cache", file=sys.stderr)


def parse_workspace_toml(workspace_dir: Path) -> Dict[str, Dict]:
    """
    Parse workspace Cargo.toml for dependency versions.

    Returns dict mapping crate name to TOML specification:
    {
        "crate_name": {
            "version": "1.0.0",  # The version constraint
            "features": [...],   # Optional features
            "optional": bool,    # If optional
        }
    }
    """
    cargo_toml_path = workspace_dir / "Cargo.toml"

    with open(cargo_toml_path, "rb") as f:
        cargo_toml = tomllib.load(f)

    workspace_deps = cargo_toml.get("workspace", {}).get("dependencies", {})

    dependencies = {}

    for name, spec in workspace_deps.items():
        if isinstance(spec, str):
            # Simple version string: crate = "1.0.0"
            dependencies[name] = {
                "version": spec,
                "features": [],
                "optional": False,
            }
        elif isinstance(spec, dict):
            # Complex spec: crate = { version = "1.0.0", features = [...] }
            version = spec.get("version")

            # Handle path/git dependencies (no version from crates.io)
            if version is None:
                if "path" in spec:
                    version = f"path:{spec['path']}"
                elif "git" in spec:
                    version = f"git:{spec.get('tag', spec.get('branch', 'main'))}"
                else:
                    version = "unknown"

            dependencies[name] = {
                "version": version,
                "features": spec.get("features", []),
                "optional": spec.get("optional", False),
            }

    return dependencies


def version_matches_constraint(version: str, constraint: str) -> bool:
    """
    Check if a version matches a Cargo version constraint.

    Simplified matching - checks if major version matches.
    Handles common cases like "2.1.0", "^2.1", "~2.1", "2", "0.4.0-alpha.7"
    """
    # Extract major version from constraint
    constraint = constraint.lstrip("^~>=<")  # Remove common prefixes

    # Handle pre-release versions like "0.4.0-alpha.7"
    constraint_base = constraint.split("-")[0] if "-" in constraint else constraint
    version_base = version.split("-")[0] if "-" in version else version

    # Split into parts
    constraint_parts = constraint_base.split(".")
    version_parts = version_base.split(".")

    if not constraint_parts or not version_parts:
        return False

    # For 0.x versions, match major.minor
    if constraint_parts[0] == "0" and len(constraint_parts) > 1:
        return (
            version_parts[0] == constraint_parts[0]
            and len(version_parts) > 1
            and version_parts[1] == constraint_parts[1]
        )

    # For 1.x+ versions, match major version
    return version_parts[0] == constraint_parts[0]


def build_report(
    toml_deps: Dict[str, Dict],
    metadata: Dict[str, Dict],
    updates: Dict[str, Dict],
    crates_io: Dict[str, Optional[str]],
) -> List[Dict]:
    """
    Join all data sources into a single report.

    Returns list of dicts, one per dependency:
    {
        "name": "crate_name",
        "toml_version": "1.0.0",      # From workspace Cargo.toml
        "locked_version": "1.0.5",    # From Cargo.lock (via metadata)
        "compatible_version": "1.2.0", # From cargo update --dry-run
        "latest_version": "2.0.0",    # From crates.io
    }
    """
    report = []

    # Only report on workspace dependencies (what we control)
    for name, toml_info in sorted(toml_deps.items()):
        toml_version = toml_info["version"]

        # Skip path dependencies (our own crates)
        is_workspace_crate = toml_version.startswith("path:")
        if is_workspace_crate:
            continue

        # Get locked version from metadata
        locked_version = None
        if name in metadata:
            versions = metadata[name]
            # If multiple versions, find the one matching our TOML constraint
            registry_versions = [v for v in versions if v["source"] == "registry"]

            # First, try to find a version matching our constraint
            for v in registry_versions:
                if version_matches_constraint(v["version"], toml_version):
                    locked_version = v["version"]
                    break

            # Fallback: pick the highest version if no match found
            if locked_version is None and registry_versions:
                # Sort by version (simple string sort works for semver in most cases)
                registry_versions.sort(key=lambda x: x["version"], reverse=True)
                locked_version = registry_versions[0]["version"]

            # Final fallback: any version
            if locked_version is None and versions:
                locked_version = versions[0]["version"]

        # Get compatible version from cargo update
        compatible_version = None
        if name in updates:
            update_info = updates[name]
            if update_info["status"] == "updating":
                compatible_version = update_info["compatible"]
            # Note: "unchanged" status means already at latest compatible,
            # so we leave compatible_version as None (will show "-")

        # Validate: compatible version must match TOML constraint
        # (handles multi-version crates where cargo update reports a different version)
        if compatible_version and not version_matches_constraint(compatible_version, toml_version):
            compatible_version = None

        # If compatible == locked, show "-" (no update available)
        if compatible_version and compatible_version == locked_version:
            compatible_version = None

        # Get latest from crates.io
        latest_version = crates_io.get(name)

        report.append(
            {
                "name": name,
                "toml_version": toml_version,
                "locked_version": locked_version or "-",
                "compatible_version": compatible_version or "-",
                "latest_version": latest_version or "-",
            }
        )

    return report


def output_table(report: List[Dict], write_fn=None) -> None:
    """Output report as a formatted table."""
    if write_fn is None:
        write_fn = print

    # Calculate column widths
    headers = ["Crate", "TOML", "Locked", "Compatible", "Latest"]
    widths = [len(h) for h in headers]

    for row in report:
        widths[0] = max(widths[0], len(row["name"]))
        widths[1] = max(widths[1], len(row["toml_version"]))
        widths[2] = max(widths[2], len(row["locked_version"]))
        widths[3] = max(widths[3], len(row["compatible_version"]))
        widths[4] = max(widths[4], len(row["latest_version"]))

    # Print header
    header_fmt = " | ".join(f"{{:<{w}}}" for w in widths)
    separator = "-+-".join("-" * w for w in widths)

    write_fn(header_fmt.format(*headers))
    write_fn(separator)

    # Print rows
    for row in report:
        write_fn(
            header_fmt.format(
                row["name"],
                row["toml_version"],
                row["locked_version"],
                row["compatible_version"],
                row["latest_version"],
            )
        )


def output_csv(report: List[Dict], write_fn=None) -> None:
    """Output report as CSV."""
    if write_fn is None:
        write_fn = print

    output = io.StringIO()
    writer = csv.DictWriter(
        output,
        fieldnames=[
            "name",
            "toml_version",
            "locked_version",
            "compatible_version",
            "latest_version",
        ],
        lineterminator="\n",  # Use Unix line endings
    )
    writer.writeheader()
    writer.writerows(report)
    # Strip trailing newline since write_fn adds one
    write_fn(output.getvalue().rstrip())


def main():
    parser = argparse.ArgumentParser(
        description="Generate dependency version report",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Modes:
  default     Run cargo commands fresh, use crates.io cache + fetch missing
  cache-only  Use cached data only, no commands or network (shows "-" if missing)
  refresh     Ignore cache, fetch everything fresh

Examples:
  %(prog)s                      # Default mode
  %(prog)s --mode cache-only    # Use only cached data
  %(prog)s --mode refresh       # Force refresh all data
  %(prog)s --output csv         # Output as CSV
        """,
    )
    parser.add_argument(
        "--mode",
        choices=["default", "cache-only", "refresh"],
        default="default",
        help="Fetch mode (default: default)",
    )
    parser.add_argument(
        "--output",
        choices=["table", "json", "csv"],
        default="table",
        help="Output format (default: table)",
    )
    parser.add_argument(
        "--cache-dir",
        type=Path,
        default=Path("/tmp/dependency_report"),
        help="Directory to cache intermediate files",
    )
    parser.add_argument(
        "--limit",
        type=int,
        default=None,
        help="Limit number of crates.io API calls (for testing)",
    )
    parser.add_argument(
        "--output-file",
        "-o",
        type=Path,
        default=None,
        help="Write output to file instead of stdout",
    )

    args = parser.parse_args()

    print("=" * 60, file=sys.stderr)
    print("🔍 Dependency Version Report", file=sys.stderr)
    print(f"   Mode: {args.mode}", file=sys.stderr)
    print("=" * 60, file=sys.stderr)

    # Ensure cache directory exists
    args.cache_dir.mkdir(parents=True, exist_ok=True)

    # Define cache file paths
    update_file = args.cache_dir / "cargo_update_dry_run.txt"
    metadata_file = args.cache_dir / "cargo_metadata.json"

    # Step 1 & 2: Handle cargo commands based on mode
    if args.mode == "cache-only":
        # Use cached data only
        print("\n📦 Using cached data only...", file=sys.stderr)
        if not update_file.exists():
            print("  ⚠ No cached cargo update data", file=sys.stderr)
        if not metadata_file.exists():
            print("  ⚠ No cached cargo metadata", file=sys.stderr)
    else:
        # Run cargo commands (both default and refresh modes)
        result = fetch_cargo_update_dry_run(args.cache_dir)
        if result is not None:
            update_file = result
        metadata_file = fetch_cargo_metadata(args.cache_dir)

    # Parse cargo data
    print("\n📦 Parsing data sources...", file=sys.stderr)

    updates = {}
    if update_file.exists():
        updates = parse_cargo_update_output(update_file)
        print(f"  ✓ Cargo update: {len(updates)} crates with update info", file=sys.stderr)
    else:
        print("  - Cargo update: no data", file=sys.stderr)

    metadata = {}
    if metadata_file.exists():
        metadata = parse_cargo_metadata(metadata_file)
        print(f"  ✓ Cargo metadata: {len(metadata)} unique crates", file=sys.stderr)
    else:
        print("  - Cargo metadata: no data", file=sys.stderr)

    # Step 3: Parse workspace Cargo.toml (always fresh - it's just a local file read)
    workspace_dir = Path.cwd()
    toml_deps = parse_workspace_toml(workspace_dir)
    print(f"  ✓ Workspace TOML: {len(toml_deps)} dependencies", file=sys.stderr)

    # Step 4: Handle crates.io based on mode
    crates_io = {}
    external_deps = [
        name for name, info in toml_deps.items() if not info["version"].startswith("path:")
    ]
    if args.limit:
        external_deps = external_deps[: args.limit]

    if args.mode == "cache-only":
        # Use cache only, no fetching
        print("\n🌐 Loading crates.io cache...", file=sys.stderr)
        crates_io = load_crates_io_cache(args.cache_dir)
    elif args.mode == "refresh":
        # Clear cache and fetch fresh
        print("\n🌐 Refreshing crates.io data...", file=sys.stderr)
        clear_crates_io_cache(args.cache_dir)
        print(
            f"  Fetching {len(external_deps)} crate versions from crates.io...",
            file=sys.stderr,
        )
        crates_io = fetch_crates_io_versions(
            external_deps,
            args.cache_dir,
            use_cache=False,  # Don't use cache in refresh mode
            rate_limit_delay=0.05,
        )
    else:
        # Default: use cache + fetch missing
        print(
            f"\n🌐 Fetching {len(external_deps)} crate versions from crates.io...",
            file=sys.stderr,
        )
        crates_io = fetch_crates_io_versions(
            external_deps,
            args.cache_dir,
            use_cache=True,
            rate_limit_delay=0.05,
        )

    # Step 5: Build and output report
    print("\n📊 Building report...", file=sys.stderr)
    report = build_report(toml_deps, metadata, updates, crates_io)
    print(f"  ✓ Report contains {len(report)} dependencies\n", file=sys.stderr)

    # Calculate summary stats
    # Compatible update = compatible_version is set and differs from locked
    has_compatible_update = sum(1 for r in report if r["compatible_version"] != "-")
    # Major update = latest differs from both locked and compatible (excludes path deps already)
    has_major_update = sum(
        1
        for r in report
        if r["latest_version"] not in ("-", r["compatible_version"], r["locked_version"])
    )

    # Collect output into a list, then write
    output_lines: List[str] = []

    def write_output(text: str):
        output_lines.append(text)

    # Generate output in requested format
    if args.output == "table":
        output_table(report, write_fn=write_output)
        # Add summary to table output
        write_output("")
        write_output("=" * 60)
        write_output("SUMMARY")
        write_output("=" * 60)
        write_output(f"Total dependencies in workspace: {len(report)}")
        write_output(f"  - With semver-compatible updates: {has_compatible_update}")
        write_output(f"  - With major version updates: {has_major_update}")
    elif args.output == "csv":
        output_csv(report, write_fn=write_output)
    elif args.output == "json":
        json_output = {
            "dependencies": report,
            "summary": {
                "total": len(report),
                "compatible_updates": has_compatible_update,
                "major_updates": has_major_update,
            },
        }
        write_output(json.dumps(json_output, indent=2))

    # Write to file or stdout
    final_output = "\n".join(output_lines)
    if args.output_file:
        print(f"  Writing to {args.output_file}", file=sys.stderr)
        with open(args.output_file, "w") as f:
            f.write(final_output + "\n")
    else:
        print(final_output)

    # Exit with code indicating updates available (useful for CI)
    # 0 = all up to date, 1 = compatible updates available
    if has_compatible_update > 0:
        sys.exit(1)
    sys.exit(0)


if __name__ == "__main__":
    main()
