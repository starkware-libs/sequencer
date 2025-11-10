import argparse
import csv
import sys
from pathlib import Path
from typing import List

import plotly.graph_objects as go


class BenchmarkData:
    def __init__(self, csv_file: str):
        """Create benchmark plot from a single CSV file."""
        file = Path(csv_file)
        if not file.exists():
            raise FileNotFoundError(f"CSV file not found: {file}")

        self.file_stem = file.stem

        # Read CSV data
        try:
            with open(file, "r", newline="") as csvfile:
                reader = csv.DictReader(csvfile)
                data = list(reader)
        except Exception as e:
            raise ValueError(f"Error reading CSV file {file}: {e}")

        # Extract and convert data to appropriate types
        try:
            self.initial_facts_in_db = [int(row["initial_facts_in_db"]) for row in data]
            self.block_duration_millis = [float(row["block_duration_millis"]) for row in data]
            self.read_duration_millis = [float(row["read_duration_millis"]) for row in data]
            self.compute_duration_millis = [float(row["compute_duration_millis"]) for row in data]
            self.write_duration_millis = [float(row["write_duration_millis"]) for row in data]
            self.n_new_facts = [int(row["n_new_facts"]) for row in data]
            self.n_read_facts = [int(row["n_read_facts"]) for row in data]
            self.block_numbers = [int(row["block_number"]) for row in data]
        except (KeyError, ValueError) as e:
            raise ValueError(f"Error processing CSV data: missing or invalid columns: {e}")

    def _duration_per_fact(self, duration_millis: List[float], n_facts: List[int]) -> List[float]:
        return [ms * 1000 / n if n > 0 else 0 for ms, n in zip(duration_millis, n_facts)]

    def _create_figure(self) -> go.Figure:
        read_duration_per_read_fact = self._duration_per_fact(
            self.read_duration_millis, self.n_read_facts
        )
        read_duration_per_new_fact = self._duration_per_fact(
            self.read_duration_millis, self.n_new_facts
        )
        compute_duration_per_new_fact = self._duration_per_fact(
            self.compute_duration_millis, self.n_new_facts
        )
        write_duration_per_new_fact = self._duration_per_fact(
            self.write_duration_millis, self.n_new_facts
        )
        total_duration_per_new_fact = self._duration_per_fact(
            self.block_duration_millis, self.n_new_facts
        )

        fig = go.Figure()

        # Add traces for different metrics
        fig.add_trace(
            go.Scatter(
                x=self.initial_facts_in_db,
                y=self.block_duration_millis,
                mode="lines+markers",
                name="Block Duration (ms)",
            )
        )
        fig.add_trace(
            go.Scatter(
                x=self.initial_facts_in_db,
                y=read_duration_per_read_fact,
                mode="lines+markers",
                name="Read Duration per read fact (µs)",
            )
        )
        fig.add_trace(
            go.Scatter(
                x=self.initial_facts_in_db,
                y=read_duration_per_new_fact,
                mode="lines+markers",
                name="Read Duration per new fact (µs)",
            )
        )
        fig.add_trace(
            go.Scatter(
                x=self.initial_facts_in_db,
                y=compute_duration_per_new_fact,
                mode="lines+markers",
                name="Compute Duration per new fact (µs)",
            )
        )
        fig.add_trace(
            go.Scatter(
                x=self.initial_facts_in_db,
                y=write_duration_per_new_fact,
                mode="lines+markers",
                name="Write Duration per new fact (µs)",
            )
        )
        fig.add_trace(
            go.Scatter(
                x=self.initial_facts_in_db,
                y=total_duration_per_new_fact,
                mode="lines+markers",
                name="Total Duration per new fact (µs)",
            )
        )

        fig.update_traces(hoverlabel=dict(namelength=-1))  # show full trace name if included

        # Customize layout
        fig.update_layout(
            title=f"Committer Benchmark - {max(self.block_numbers)} blocks",
            xaxis_title="Initial Facts in DB",
            yaxis_title="Time per Fact",
            legend_title="Metrics",
            hovermode="x unified",
        )

        return fig

    def plot(self, output_path: str = None, output_dir: str = None):
        fig = self._create_figure()

        if output_path is not None or output_dir is not None:
            output_file = Path(
                output_path if output_path is not None else f"{output_dir}/{self.file_stem}.html"
            )
            output_file.parent.mkdir(parents=True, exist_ok=True)
            fig.write_html(str(output_file))
            print(f"Plot saved to {str(output_file)}")
        else:
            fig.show()


def create_benchmark_plot(csv_file: str, output_path: str = None, output_dir: str = None):
    """Create benchmark plot from a single CSV file."""
    data = BenchmarkData(csv_file=csv_file)
    data.plot(output_path=output_path, output_dir=output_dir)


def main():
    parser = argparse.ArgumentParser(
        description="Generate interactive plot from a committer benchmark CSV file"
    )
    parser.add_argument("csv_file", type=str, help="Path to CSV file to plot")
    parser.add_argument(
        "-o", "--output", type=str, help="Output HTML file path (default: show in browser)"
    )
    parser.add_argument(
        "-d",
        "--output-dir",
        type=str,
        help="Output directory for the plot with the name <csv_file_name>.html.  \
            Ignored if --output is provided.",
    )

    args = parser.parse_args()

    try:
        create_benchmark_plot(
            csv_file=args.csv_file, output_path=args.output, output_dir=args.output_dir
        )
    except Exception as e:
        print(f"Error creating plot: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
