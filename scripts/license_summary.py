import sys
import argparse
import tomli
import requests
from typing import Dict, List, Optional
from collections import Counter


def parse_args(args):
    parser = argparse.ArgumentParser(
        description="Get the license information from crates.io for Cargo dependencies",
    )
    parser.add_argument(
        "--toml_file",
        type=str,
        required=True,
        help="Path to the Cargo.toml file",
    )
    parser.add_argument(
        "--verbose",
        action="store_true",
        help="Show detailed progress information",
    )
    parser.add_argument(
        "--output",
        type=str,
        default=None,
        help="Output file path (default: print to console only)",
    )
    return parser.parse_args(args)


def get_external_dependencies(toml_file: str) -> List[str]:
    """Extract external (non-internal) dependencies from Cargo.toml."""
    with open(toml_file, "rb") as f:
        toml_data = tomli.load(f)
    dependencies = toml_data.get("workspace", {}).get("dependencies", {})
    return [
        k
        for k in dependencies.keys()
        if not k.startswith("apollo_") and not k.startswith("starknet_")
    ]


def get_license(crates: List[str], verbose: bool = False) -> Dict[str, Optional[str]]:
    """Fetch license information for each crate from crates.io API."""
    licenses = {}
    headers = {"User-Agent": "cargo-license-checker (https://github.com/starkware)"}
    total_crates = len(crates)

    for idx, crate in enumerate(crates, 1):
        if verbose:
            print(
                f"[{idx}/{total_crates}] Fetching license for: {crate}", file=sys.stderr
            )

        url = f"https://crates.io/api/v1/crates/{crate}"
        try:
            response = requests.get(url, headers=headers, timeout=10)
            response.raise_for_status()

            data = response.json()
            versions = data.get("versions", [])

            if versions:
                license_info = versions[0].get("license")
                licenses[crate] = license_info if license_info else "Unknown"
            else:
                licenses[crate] = "No versions found"

        except requests.exceptions.RequestException as e:
            licenses[crate] = f"Error: {e}"
            if verbose:
                print(f"  Failed to fetch {crate}: {e}", file=sys.stderr)

    return licenses


def format_licenses(licenses: Dict[str, Optional[str]]) -> str:
    """Format licenses as a sorted list."""
    if not licenses:
        return "No licenses found."

    # Sort by license type first, then by crate name
    sorted_items = sorted(licenses.items(), key=lambda x: (x[1] or "", x[0]))
    max_crate_len = max(len(crate) for crate in licenses.keys())

    formatted = []
    for crate, license_type in sorted_items:
        formatted.append(f"{crate:<{max_crate_len}}  {license_type}")

    return "\n".join(formatted)


def licenses_summary(licenses: Dict[str, Optional[str]]) -> str:
    """Generate a summary of unique licenses and their counts."""
    if not licenses:
        return "No licenses to summarize."

    license_counts = Counter(licenses.values())

    # Sort by count (descending), then by license name
    sorted_licenses = sorted(license_counts.items(), key=lambda x: (-x[1], x[0] or ""))

    summary = []
    summary.append(f"Total crates analyzed: {len(licenses)}")
    summary.append(f"Unique licenses: {len(license_counts)}")
    summary.append("\nLicense breakdown:")

    for license_type, count in sorted_licenses:
        percentage = (count / len(licenses)) * 100
        summary.append(
            f"  {license_type or 'Unknown':<30} {count:>3} crate(s) ({percentage:.1f}%)"
        )

    return "\n".join(summary)


def main():
    args = parse_args(sys.argv[1:])

    try:
        dependencies = get_external_dependencies(args.toml_file)

        if not dependencies:
            print("No external dependencies found.")
            return

        print(f"Found {len(dependencies)} external dependencies.\n")

        licenses = get_license(dependencies, verbose=args.verbose)

        # Build the output report
        separator = "=" * 80
        report_lines = []
        report_lines.append(separator)
        report_lines.append("LICENSE DETAILS")
        report_lines.append(separator)
        report_lines.append(format_licenses(licenses))
        report_lines.append("\n" + separator)
        report_lines.append("LICENSE SUMMARY")
        report_lines.append(separator)
        report_lines.append(licenses_summary(licenses))

        report = "\n".join(report_lines)

        # Print to console
        print("\n" + report)

        # Write to file if specified
        if args.output:
            with open(args.output, "w", encoding="utf-8") as f:
                f.write(report + "\n")
            print(f"\n Report saved to: {args.output}")

    except FileNotFoundError:
        print(f"Error: File '{args.toml_file}' not found.", file=sys.stderr)
        sys.exit(1)
    except KeyError as e:
        print(f"Error: Missing expected key in TOML file: {e}", file=sys.stderr)
        sys.exit(1)
    except IOError as e:
        print(f"Error writing to output file: {e}", file=sys.stderr)
        sys.exit(1)
    except Exception as e:  # pylint: disable=broad-except
        # Catch all to ensure we exit gracefully with error message
        print(f"Unexpected error: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
