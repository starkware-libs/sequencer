from __future__ import annotations

import logging
import sys
from typing import Optional

_ECHO_NET_ROOT = "echonet"
_CONFIGURED = False


class StdoutStderrRoutingHandler(logging.StreamHandler):
    """
    Route logs to stdout for <ERROR and to stderr for ERROR.
    """

    def emit(self, record: logging.LogRecord) -> None:
        self.stream = sys.stderr if record.levelno >= logging.ERROR else sys.stdout
        super().emit(record)


# Backwards-compatible alias (private name used by older code).
_StdoutStderrRoutingHandler = StdoutStderrRoutingHandler


try:
    from gunicorn.glogging import Logger as _GunicornLoggerBase  # type: ignore
except Exception:  # pragma: no cover - gunicorn may not be installed in dev envs
    _GunicornLoggerBase = object  # type: ignore[misc,assignment]


class EchoNetGunicornLogger(_GunicornLoggerBase):
    """
    Gunicorn writes many "INFO" startup lines to its error logger.

    In container environments, stderr is commonly mapped to ERROR severity by log
    ingestion (e.g., Google Cloud Logging). This logger routes records by level:
    INFO/WARNING -> stdout, ERROR+ -> stderr.
    """

    def setup(self, cfg) -> None:  # type: ignore[override]
        # When gunicorn is available, this ensures default internal initialization
        # happens before we swap handlers.
        if hasattr(super(), "setup"):
            super().setup(cfg)  # type: ignore[misc]

        formatter = logging.Formatter("[%(levelname)s] [%(name)s] %(message)s")
        handler = StdoutStderrRoutingHandler()
        handler.setFormatter(formatter)

        error_log = getattr(self, "error_log", None)
        access_log = getattr(self, "access_log", None)
        for log in (error_log, access_log):
            if log is None:
                continue
            log.handlers.clear()
            log.addHandler(handler)
            log.propagate = False

        error_level = getattr(logging, str(getattr(cfg, "loglevel", "info")).upper(), logging.INFO)
        if error_log is not None:
            error_log.setLevel(error_level)
        if access_log is not None:
            access_log.setLevel(logging.INFO)


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
        handler = StdoutStderrRoutingHandler()
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
