from __future__ import annotations

import sys
from typing import Optional

import logging

_ECHO_NET_ROOT = "echonet"
_CONFIGURED = False


def configure_logging() -> None:
    """
    Configure the `echonet` logger namespace once for the current process.

    - Uses stdout for logs (k8s-friendly).
    """
    global _CONFIGURED
    if _CONFIGURED:
        return

    logger = logging.getLogger(_ECHO_NET_ROOT)

    # Only attach our handler if nobody attached one yet. This prevents double logging
    # when modules are reloaded or multiple components import `get_logger()`.
    if not logger.handlers:
        handler = logging.StreamHandler(sys.stdout)
        formatter = logging.Formatter("[%(levelname)s] [%(name)s] %(message)s")
        handler.setFormatter(formatter)
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
