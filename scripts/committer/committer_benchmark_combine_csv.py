import argparse
import sys
from pathlib import Path

import csv
import glob
from typing import List, Optional


def unify_csvs(
    input_pattern: str,
    output_file: Optional[str] = None,
    output_dir: Optional[str] = None,
    id_column: str = "block_number",
):
    # Find and sort files by name
    files = sorted(glob.glob(input_pattern))
    if not files:
        raise ValueError(f"No CSV files found with pattern: {input_pattern}")

    all_rows = []
    fieldnames: Optional[List[str]] = None

    # Read all CSV files
    for file_path in files:
        with open(file_path, "r", newline="") as csvfile:
            reader = csv.DictReader(csvfile)

            if fieldnames is None:
                # Set fieldnames from first file
                fieldnames = reader.fieldnames
            assert fieldnames == reader.fieldnames, f"Different column names in {file_path}"

            all_rows.extend((row for row in reader))

    if not all_rows:
        raise ValueError("No data found in CSV files")

    try:
        all_rows.sort(key=lambda x: int(x[id_column]))
    except (ValueError, KeyError):
        print(f"Warning: Could not sort by {id_column}, using string sort", file=sys.stderr)
        all_rows.sort(key=lambda x: x.get(id_column, ""))

    # If no output file specified, generate one based on max block number
    if output_file is None:
        try:
            max_block = max(int(row[id_column]) for row in all_rows)
        except (ValueError, KeyError):
            max_block = "combined"

        # Use output_dir if specified, otherwise use input directory
        if output_dir is not None:
            target_dir = Path(output_dir)
            target_dir.mkdir(parents=True, exist_ok=True)
        else:
            target_dir = Path(files[0]).parent
        output_file = target_dir / f"{max_block}.csv"

    # Write combined CSV
    with open(output_file, "w", newline="") as csvfile:
        writer = csv.DictWriter(csvfile, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(all_rows)

    print(f"Combined {len(files)} CSV files ({len(all_rows)} rows) into {output_file}")


def main():
    parser = argparse.ArgumentParser(
        description="Combine multiple CSV files from committer benchmark into a single file (named \
            <max_block_number>.csv by default)"
    )
    parser.add_argument(
        "csv_path",
        type=str,
        help="Path to directory containing CSV files or glob pattern (e.g., '/tmp/benchmark/*.csv')",
    )
    parser.add_argument(
        "-o",
        "--output",
        type=str,
        help="Output file path (overrides --output-dir and default behavior)",
    )
    parser.add_argument(
        "-d",
        "--output-dir",
        type=str,
        help="Output directory for default filename (default: same directory as input files)",
    )
    args = parser.parse_args()

    # Convert path to glob pattern if it's a directory
    csv_path = Path(args.csv_path)
    if csv_path.is_dir():
        input_pattern = str(csv_path / "*.csv")
    else:
        input_pattern = args.csv_path

    try:
        unify_csvs(input_pattern, args.output, args.output_dir)
    except ValueError as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
