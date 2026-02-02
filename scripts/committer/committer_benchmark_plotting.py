import argparse
import csv
import sys
from datetime import datetime
from enum import Enum, auto
from pathlib import Path
from typing import List, Optional

import plotly.graph_objects as go

TICK_FORMAT = "%d/%m/%Y %H:%M:%S.%L"

# Mapping from old CSV column names to new ones for backward compatibility.
COLUMN_NAME_MIGRATIONS = {
    "n_read_facts": "n_reads",
    "n_new_facts": "n_writes",
    "initial_facts_in_db": "initial_db_entry_count",
}


def normalize_csv_columns(data: List[dict]) -> List[dict]:
    """Rename old CSV column names to new ones for backward compatibility."""
    if not data:
        return data

    # Check if any old column names exist in the header (first row's keys)
    header_keys = data[0].keys()
    renames_needed = {old: new for old, new in COLUMN_NAME_MIGRATIONS.items() if old in header_keys}

    if not renames_needed:
        return data

    # Rename keys in each row's dictionary
    return [{renames_needed.get(key, key): value for key, value in row.items()} for row in data]


class StorageStats(Enum):
    NONE = 0
    MDBX = auto()
    ROCKSDB = auto()
    CACHED_MDBX = auto()
    CACHED_ROCKSDB = auto()

    @staticmethod
    def cached_columns() -> List[str]:
        return ["reads", "cached reads", "writes", "cache hit rate"]

    @staticmethod
    def mdbx_columns() -> List[str]:
        return ["Page size", "Tree depth", "Branch pages", "Leaf pages", "Overflow pages"]

    @staticmethod
    def rocksdb_columns() -> List[str]:
        # RocksDB stats currently not implemented.
        return []

    def columns(self) -> List[str]:
        if self == StorageStats.MDBX:
            return self.mdbx_columns()
        elif self == StorageStats.ROCKSDB:
            return self.rocksdb_columns()
        elif self == StorageStats.CACHED_MDBX:
            return self.cached_columns() + self.mdbx_columns()
        elif self == StorageStats.CACHED_ROCKSDB:
            return self.cached_columns() + self.rocksdb_columns()
        else:
            assert self == StorageStats.NONE, f"Invalid storage stats: {self}"
            return []


class BenchmarkData:
    def __init__(self, csv_file: str, storage_stats_type: StorageStats):
        """Create benchmark plot from a single CSV file."""
        file = Path(csv_file)
        if not file.exists():
            raise FileNotFoundError(f"CSV file not found: {file}")

        self.file_stem = file.stem
        self.storage_stats_type = storage_stats_type

        # Read CSV data
        try:
            with open(file, "r", newline="") as csvfile:
                reader = csv.DictReader(csvfile)
                data = list(reader)
            # Normalize old column names for backward compatibility.
            # TODO(Rotem): remove this once all benchmarks are updated to use the new field names.
            data = normalize_csv_columns(data)
        except Exception as e:
            raise ValueError(f"Error reading CSV file {file}: {e}")

        # Extract and convert data to appropriate types
        try:
            # General data for all storage types.
            self.initial_db_entry_count = [int(row["initial_db_entry_count"]) for row in data]
            self.block_duration_millis = [float(row["block_duration_millis"]) for row in data]
            self.read_duration_millis = [float(row["read_duration_millis"]) for row in data]
            self.compute_duration_millis = [float(row["compute_duration_millis"]) for row in data]
            self.time_of_measurement = [
                datetime.fromtimestamp(int(row["time_of_measurement"]) / 1000) for row in data
            ]
            self.write_duration_millis = [float(row["write_duration_millis"]) for row in data]
            self.n_writes = [int(row["n_writes"]) for row in data]
            self.n_reads = [int(row["n_reads"]) for row in data]
            self.block_numbers = [int(row["block_number"]) for row in data]

            # Optional modification counts - may not exist in older CSV files.
            self.modifications_data: dict[str, List[int]] = {}
            modifications_columns = [
                "n_storage_tries_modifications",
                "n_contracts_trie_modifications",
                "n_classes_trie_modifications",
                "n_emptied_storage_leaves",
            ]
            for col in modifications_columns:
                if col in data[0]:
                    self.modifications_data[col] = [int(row[col]) for row in data]

            # Storage-specific data.
            # It may be the case that not every row contains storage statistics, so to keep the
            # graph nice, use the previous row values.
            columns = storage_stats_type.columns()
            self.storage_stats = {column_title: [] for column_title in columns}
            last_real_values = {column_title: 0 for column_title in columns}
            for row in data:
                row_values = {column_title: str(row[column_title]) for column_title in columns}
                if "" in row_values.values():
                    # No values in this row, use the previous row values.
                    # For sanity assert that the values are the same.
                    assert all(
                        value == "" for value in row_values.values()
                    ), "All values in the storage stats row should be empty if one is empty."
                    row_values = last_real_values
                else:
                    last_real_values = row_values
                for column_title in columns:
                    self.storage_stats[column_title].append(float(row_values[column_title]))

        except (KeyError, ValueError) as e:
            raise ValueError(f"Error processing CSV data: missing or invalid columns: {e}")

    def _duration_per_entry(
        self, duration_millis: List[float], n_entries: List[int]
    ) -> List[float]:
        return [ms * 1000 / n if n > 0 else 0 for ms, n in zip(duration_millis, n_entries)]

    def _update_figure_layout(self, figure: go.Figure, title: str):
        figure.update_layout(
            title=f"Committer Benchmark - {max(self.block_numbers)} blocks, storage type {self.storage_stats_type.name}.",
            xaxis_title="Time of Measurement",
            xaxis=dict(tickformat=TICK_FORMAT),
            yaxis_title=title,
            legend_title="Metrics",
            hovermode="x unified",
        )

    def create_durations_figure(self) -> go.Figure:
        read_duration_per_read_entry = self._duration_per_entry(
            self.read_duration_millis, self.n_reads
        )
        read_duration_per_write_entry = self._duration_per_entry(
            self.read_duration_millis, self.n_writes
        )
        compute_duration_per_write_entry = self._duration_per_entry(
            self.compute_duration_millis, self.n_writes
        )
        write_duration_per_write_entry = self._duration_per_entry(
            self.write_duration_millis, self.n_writes
        )
        total_duration_per_write_entry = self._duration_per_entry(
            self.block_duration_millis, self.n_writes
        )

        durations_figure = go.Figure()

        # Add traces for different metrics
        durations_figure.add_trace(
            go.Scatter(
                x=self.time_of_measurement,
                y=self.block_duration_millis,
                mode="lines+markers",
                name="Block Duration (ms)",
            )
        )
        durations_figure.add_trace(
            go.Scatter(
                x=self.time_of_measurement,
                y=read_duration_per_read_entry,
                mode="lines+markers",
                name="Read Duration per read entry (µs)",
            )
        )
        durations_figure.add_trace(
            go.Scatter(
                x=self.time_of_measurement,
                y=read_duration_per_write_entry,
                mode="lines+markers",
                name="Read Duration per write entry (µs)",
            )
        )

        durations_figure.add_trace(
            go.Scatter(
                x=self.time_of_measurement,
                y=compute_duration_per_write_entry,
                mode="lines+markers",
                name="Compute Duration per write entry (µs)",
            )
        )
        durations_figure.add_trace(
            go.Scatter(
                x=self.time_of_measurement,
                y=write_duration_per_write_entry,
                mode="lines+markers",
                name="Write Duration per write entry (µs)",
            )
        )
        durations_figure.add_trace(
            go.Scatter(
                x=self.time_of_measurement,
                y=total_duration_per_write_entry,
                mode="lines+markers",
                name="Total Duration per write entry (µs)",
            )
        )

        durations_figure.update_traces(
            hoverlabel=dict(namelength=-1)
        )  # show full trace name if included

        # Customize layout
        self._update_figure_layout(durations_figure, "Time per DB Entry")

        return durations_figure

    def create_total_db_entries_figure(self) -> go.Figure:
        total_db_entries_figure = go.Figure()
        total_db_entries_figure.add_trace(
            go.Scatter(
                x=self.time_of_measurement,
                y=self.initial_db_entry_count,
                mode="lines+markers",
                name="Total entries in the DB",
            )
        )
        total_db_entries_figure.update_traces(
            hoverlabel=dict(namelength=-1)
        )  # show full trace name if included
        self._update_figure_layout(total_db_entries_figure, "Total entries in the DB")
        return total_db_entries_figure

    def create_storage_stats_figure(self) -> go.Figure:
        storage_stats_figure = go.Figure()
        for column_title, values in self.storage_stats.items():
            storage_stats_figure.add_trace(
                go.Scatter(
                    x=self.time_of_measurement,
                    y=values,
                    mode="lines+markers",
                    name=column_title,
                )
            )
        storage_stats_figure.update_traces(hoverlabel=dict(namelength=-1))
        self._update_figure_layout(storage_stats_figure, "Storage stats")
        return storage_stats_figure

    def create_modifications_figure(self) -> go.Figure:
        modifications_figure = go.Figure()
        for column_title, values in self.modifications_data.items():
            modifications_figure.add_trace(
                go.Scatter(
                    x=self.time_of_measurement,
                    y=values,
                    mode="lines+markers",
                    name=column_title,
                )
            )
        modifications_figure.update_traces(hoverlabel=dict(namelength=-1))
        self._update_figure_layout(modifications_figure, "Tries Modifications Count")
        return modifications_figure

    def plot(self, output_dir: Optional[str] = None):
        durations_figure = self.create_durations_figure()
        total_db_entries_figure = self.create_total_db_entries_figure()
        if self.storage_stats_type != StorageStats.NONE:
            storage_stats_figure = self.create_storage_stats_figure()
        if self.modifications_data:
            modifications_figure = self.create_modifications_figure()

        if output_dir is not None:
            durations_output_file = Path(f"{output_dir}/{self.file_stem}_durations.html")
            total_db_entries_output_file = Path(
                f"{output_dir}/{self.file_stem}_total_db_entries.html"
            )
            storage_stats_output_file = Path(f"{output_dir}/{self.file_stem}_storage_stats.html")
            modifications_output_file = Path(f"{output_dir}/{self.file_stem}_modifications.html")
            durations_output_file.parent.mkdir(parents=True, exist_ok=True)

            durations_figure.write_html(str(durations_output_file))
            total_db_entries_figure.write_html(str(total_db_entries_output_file))
            if self.storage_stats_type != StorageStats.NONE:
                storage_stats_figure.write_html(str(storage_stats_output_file))
            if self.modifications_data:
                modifications_figure.write_html(str(modifications_output_file))
                print(f"Plot saved to {str(modifications_output_file)}")
            print(f"Plot saved to {str(durations_output_file)}")
            print(f"Plot saved to {str(total_db_entries_output_file)}")
        else:
            durations_figure.show()
            total_db_entries_figure.show()
            if self.storage_stats_type != StorageStats.NONE:
                storage_stats_figure.show()
            if self.modifications_data:
                modifications_figure.show()


def create_benchmark_plot(
    csv_file: str, output_dir: Optional[str], storage_stats_type: StorageStats
):
    """Create benchmark plot from a single CSV file."""
    data = BenchmarkData(csv_file=csv_file, storage_stats_type=storage_stats_type)
    data.plot(output_dir=output_dir)


def main():
    parser = argparse.ArgumentParser(
        description="Generate interactive plot from a committer benchmark CSV file"
    )
    parser.add_argument("csv_file", type=str, help="Path to CSV file to plot")
    parser.add_argument(
        "-d",
        "--output-dir",
        type=str,
        default=None,
        help="Output directory for the plot with the name <csv_file_name>.html. \
            If not provided, opens a browser with the plots.",
    )
    parser.add_argument(
        "-s",
        "--storage-stats",
        type=str,
        choices=[storage_type.name for storage_type in StorageStats],
        default=StorageStats.NONE.name,
        help="Storage stats to plot",
    )

    args = parser.parse_args()

    try:
        create_benchmark_plot(
            csv_file=args.csv_file,
            output_dir=args.output_dir,
            storage_stats_type=StorageStats[args.storage_stats],
        )
    except Exception as e:
        print(f"Error creating plot: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
