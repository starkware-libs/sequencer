import json
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, FrozenSet


def rpc_response(result: Any) -> dict:
    """Wraps a result in a JSON-RPC 2.0 response."""
    return {"jsonrpc": "2.0", "id": "1", "result": result}


def timestamp_to_iso(timestamp: int) -> str:
    """Convert Unix timestamp to ISO 8601 string (UTC)."""
    return datetime.fromtimestamp(timestamp, tz=timezone.utc).isoformat().replace("+00:00", "Z")


def format_hex(value: int, width: int = 64) -> str:
    """Formats an integer as a 0x-prefixed hex string, zero-padded to width hex chars."""
    return f"0x{value:0{width}x}"


def utc_now() -> datetime:
    return datetime.now(tz=timezone.utc)


def parse_csv_to_lower_set(raw: str) -> FrozenSet[str]:
    return frozenset(part.strip().lower() for part in str(raw).split(",") if part.strip())


def read_json_object(path: Path) -> dict[str, Any]:
    """
    Read a JSON file and ensure the top-level value is an object.
    """
    with open(path, "r", encoding="utf-8") as f:
        obj = json.load(f)
    if not isinstance(obj, dict):
        raise ValueError(f"Expected JSON object in {path}")
    return obj
