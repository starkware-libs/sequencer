from __future__ import annotations

import logging
import sys
from typing import Optional

_ECHO_NET_ROOT = "echonet"
_CONFIGURED = False


class _StdoutStderrRoutingHandler(logging.StreamHandler):
    """
    Route logs to stdout for <ERROR and to stderr for ERROR.
    """

    def emit(self, record: logging.LogRecord) -> None:
        self.stream = sys.stderr if record.levelno >= logging.ERROR else sys.stdout
        super().emit(record)


def configure_logging() -> None:
    """
    Configure the `echonet` logger namespace once for the current process.

    - Uses stdout for non-error logs and stderr for ERROR.
    """
    global _CONFIGURED
    if _CONFIGURED:
        return

    logger = logging.getLogger(_ECHO_NET_ROOT)

    # Only attach our handler if nobody attached one yet. This prevents double logging
    # when modules are reloaded or multiple components import `get_logger()`.
    if not logger.handlers:
        handler = _StdoutStderrRoutingHandler()
        handler.setFormatter(logging.Formatter("[%(levelname)s] [%(name)s] %(message)s"))
        logger.addHandler(handler)

    logger.setLevel(logging.DEBUG)
    logger.propagate = False
    _CONFIGURED = True


def get_logger(name: Optional[str] = None) -> logging.Logger:
    """Return a logger under the `echonet` namespace (e.g. `echonet.echo_center`)."""
    configure_logging()
    return (
        logging.getLogger(f"{_ECHO_NET_ROOT}.{name}") if name else logging.getLogger(_ECHO_NET_ROOT)
    )
