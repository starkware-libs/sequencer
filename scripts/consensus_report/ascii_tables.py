"""
ASCII table formatting utilities.

Provides functions to render data as ASCII tables with borders:
- ascii_table: standard table with headers and data rows
- ascii_table_with_spanner: table with a label spanning multiple columns
"""

from typing import List, Optional


def determine_column_widths(headers: List[str], rows: List[List[str]]) -> List[int]:
    """Calculate the width needed for each column based on headers and row data."""
    widths = [len(str(header)) for header in headers]
    for row in rows:
        for i, cell in enumerate(row):
            if i < len(widths):
                widths[i] = max(widths[i], len(str(cell)))
    return widths


def ascii_table(
    headers: List[str], rows: List[List[str]], aligns: Optional[List[str]] = None
) -> str:
    """
    Render ASCII table.
    headers: list of header strings.
    rows:    list of lists of row data values.
    aligns:  per-column alignment list where "r" = right-justify, "c" = center,
             and any other value (e.g. "l") = left-justify. One entry per column.
             If omitted, all columns default to left-justified.
    """
    aligns = aligns or ["l"] * len(headers)
    widths = determine_column_widths(headers, rows)

    def pad(cell: str, width: int, align: str) -> str:
        s = str(cell)
        if align == "r":
            return s.rjust(width)
        if align == "c":
            return s.center(width)
        return s.ljust(width)

    sep = "+" + "+".join("-" * (width + 2) for width in widths) + "+"
    out = [
        sep,
        "|"
        + "|".join(" " + pad(header, width, "c") + " " for header, width in zip(headers, widths))
        + "|",
        sep,
    ]
    for row in rows:
        out.append(
            "|"
            + "|".join(
                " " + pad(cell, width, align) + " "
                for cell, width, align in zip(row, widths, aligns)
            )
            + "|"
        )
    out.append(sep)
    return "\n".join(out)


def ascii_table_with_spanner(
    spanner: str,
    left_headers: List[str],
    right_headers: List[str],
    rows: List[List[str]],
    aligns: Optional[List[str]] = None,
) -> str:
    """
    Render ASCII table with a spanner label over the right columns.
    spanner:       text to center over the right-column section.
    left_headers:  headers for left columns (no spanner).
    right_headers: headers for right columns (under the spanner).
    rows:          data rows, one list per row with values for all columns.
    aligns:        per-column alignment list where "r" = right-justify, "c" = center,
                   and any other value (e.g. "l") = left-justify. One entry per column.
                   If omitted, all columns default to left-justified.
    """
    headers = left_headers + right_headers
    aligns = aligns or ["l"] * len(headers)
    widths = determine_column_widths(headers, rows)
    sep = "+" + "+".join("-" * (width + 2) for width in widths) + "+"

    r_start = len(left_headers)
    total_right = sum((width + 2) for width in widths[r_start:]) + (len(right_headers) - 1)
    sp_cell = " " + spanner.center(max(0, total_right - 2)) + " "

    out = [sep]
    row_a = "|"
    for i in range(len(left_headers)):
        row_a += " " + "".ljust(widths[i]) + " |"
    row_a += sp_cell + "|"
    out.append(row_a)

    row_b = (
        "|"
        + "|".join(" " + header.center(width) + " " for header, width in zip(headers, widths))
        + "|"
    )
    out.append(row_b)
    out.append(sep)

    for row in rows:
        out.append(
            "|"
            + "|".join(
                " " + (str(cell).rjust(width) if align == "r" else str(cell).ljust(width)) + " "
                for cell, width, align in zip(row, widths, aligns)
            )
            + "|"
        )
    out.append(sep)
    return "\n".join(out)
