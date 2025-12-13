import sys
from typing import Optional

import logging

_ROOT_LOGGER_NAME = "echonet"
_CONFIGURED = False


def _configure_root_logger() -> None:
    """
    Configure a simple process-wide logger for the echonet package.

    Format example:
        [INFO] [echonet.manage_sequencer] Scaling to 1 replicas done.
    """
    global _CONFIGURED
    if _CONFIGURED:
        return

    logger = logging.getLogger(_ROOT_LOGGER_NAME)

    if not logger.handlers:
        handler = logging.StreamHandler(sys.stdout)
        formatter = logging.Formatter("[%(levelname)s] [%(name)s] %(message)s")
        handler.setFormatter(formatter)
        logger.addHandler(handler)

    logger.setLevel(logging.INFO)
    logger.propagate = False
    _CONFIGURED = True


def get_logger(name: Optional[str] = None) -> logging.Logger:
    """
    Return a child logger under the common 'echonet' namespace.

    Example:
        logger = get_logger("manage_sequencer")
        logger.info("Scaling to 1 replica")
    """
    _configure_root_logger()
    if name:
        return logging.getLogger(f"{_ROOT_LOGGER_NAME}.{name}")
    return logging.getLogger(_ROOT_LOGGER_NAME)


def get_flask_logger() -> logging.Logger:
    """
    Convenience logger for Flask request logs.

    Example output:
        [INFO] [echonet.flask] WRITE_BLOB len=123 ct=application/json
    """
    return get_logger("flask")
