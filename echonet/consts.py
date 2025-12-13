import os
from pathlib import Path


def _get_required_int_env(var_name: str) -> int:
    """Return an integer parsed from a required environment variable.

    This is used for values that should not be hardcoded in the source,
    such as the default starting block.
    """
    raw = os.getenv(var_name)
    if raw is None or raw == "":
        raise RuntimeError(f"Missing required environment variable: {var_name}")
    try:
        return int(raw)
    except ValueError as exc:
        raise RuntimeError(
            f"Environment variable {var_name} must be an integer, got {raw!r}"
        ) from exc


def _get_optional_int_env(var_name: str, default: int) -> int:
    """Return an integer from an environment variable, or a default if unset/empty.

    Uses the same validation and error semantics as _get_required_int_env for
    non-empty values.
    """
    raw = os.getenv(var_name)
    if raw is None or raw == "":
        return default
    return _get_required_int_env(var_name)


# Shared throttling headers used for feeder requests.
# The X-Throttling-Bypass value is injected at deploy-time via the
# FEEDER_X_THROTTLING_BYPASS environment variable
_FEEDER_X_THROTTLING_BYPASS = os.getenv("FEEDER_X_THROTTLING_BYPASS")
if _FEEDER_X_THROTTLING_BYPASS:
    FEEDER_HEADERS = {"X-Throttling-Bypass": _FEEDER_X_THROTTLING_BYPASS}
else:
    FEEDER_HEADERS = {}


# Shared base URLs
FEEDER_BASE_URL = "https://feeder.alpha-mainnet.starknet.io"
SEQUENCER_BASE_URL_DEFAULT = "http://sequencer-node-service:8080"

# Shared HTTP status codes
HTTP_OK = 200
HTTP_BAD_REQUEST = 400
HTTP_NOT_FOUND = 404


# Shared default starting block number (used by multiple apps)
# START_BLOCK_DEFAULT is injected at deploy-time from the START_BLOCK_DEFAULT
# environment variable (see deploy-echonet.sh).
START_BLOCK_DEFAULT = _get_required_int_env("START_BLOCK_DEFAULT")
END_BLOCK_DEFAULT = None
SLEEP_BETWEEN_BLOCKS_SECONDS_DEFAULT = 2.0
# Number of initial blocks from START_BLOCK_DEFAULT to apply extra sleep
INITIAL_SLOW_BLOCKS_COUNT = 10
EXTRA_SLEEP_TIME_SECONDS = 3.0


# Feeder endpoints
GET_BLOCK_ENDPOINT = "/feeder_gateway/get_block"
GET_STATE_UPDATE_ENDPOINT = "/feeder_gateway/get_state_update"
GET_SIGNATURE_ENDPOINT = "/feeder_gateway/get_signature"
GET_TRANSACTION_ENDPOINT = "/feeder_gateway/get_transaction"
GET_CLASS_BY_HASH_ENDPOINT = "/feeder_gateway/get_class_by_hash"

# Sequencer endpoints
ADD_TX_ENDPOINT = "/gateway/add_transaction"

# Shared log directory for auxiliary files (not block storage)
LOG_DIR = Path("/data/echonet")

# List of blocked sender addresses for transaction filtering.
BLOCKED_SENDERS = set()

# Minimum number of errors (gateway errors + not-committed txs)
# required before triggering a resync.
RESYNC_ERROR_THRESHOLD = _get_optional_int_env("RESYNC_ERROR_THRESHOLD", default=1)
