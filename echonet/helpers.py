from datetime import datetime, timezone
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
