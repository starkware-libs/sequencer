import argparse
from pathlib import Path
import pandas as pd
import glob
import sys

def unify_csvs(
    input_pattern: str,
    output_file: str = None, 
    output_dir: str = None, 
    id_column: str = "block_number"
):
    # Find and sort files by name
    files = sorted(glob.glob(input_pattern))
    if not files:
        raise ValueError(f"No CSV files found with pattern: {input_pattern}")

    dfs = []
    for f in files:
        df = pd.read_csv(f)
        dfs.append(df)

    # Concatenate all
    combined = pd.concat(dfs, ignore_index=True)

    # Sort by running id to enforce correct ordering
    combined = combined.sort_values(by=id_column)

    # If no output file specified, generate one based on max block number
    if output_file is None:
        max_block = combined[id_column].max()
        # Use output_dir if specified, otherwise use input directory
        if output_dir:
            target_dir = Path(output_dir)
            target_dir.mkdir(parents=True, exist_ok=True)
        else:
            target_dir = Path(files[0]).parent
        output_file = target_dir / f"{max_block}.csv"
    
    # Write once with header
    combined.to_csv(output_file, index=False)
    print(f"Combined {len(files)} CSV files into {output_file}")
    
def main():
    parser = argparse.ArgumentParser(
        description="Combine multiple CSV files from committer benchmark into a single file (named \
            <max_block_number>.csv by default)"
    )
    parser.add_argument(
        "csv_path", 
        type=str,
        help="Path to directory containing CSV files or glob pattern (e.g., '/tmp/benchmark/*.csv')"
    )
    parser.add_argument(
        "-o", "--output", 
        type=str,
        help="Output file path (overrides --output-dir and default behavior)"
    )
    parser.add_argument(
        "-d", "--output-dir", 
        type=str,
        help="Output directory for default filename (default: same directory as input files)"
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
