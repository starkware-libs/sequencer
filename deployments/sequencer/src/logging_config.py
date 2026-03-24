"""Shared logging setup using Rich for formatted, colored output.

Use get_logger(__name__) in any module. Configure once at app entry (e.g. in main())
via configure_logging() so logs show timestamp, level (e.g. yellow WARNING), and message.
"""

import logging
import sys

from rich.console import Console
from rich.logging import RichHandler


def configure_logging(
    level: int = logging.INFO,
    *,
    show_path: bool = True,
    log_time_format: str = "%Y-%m-%d %H:%M:%S",
) -> None:
    """Configure the root logger with a Rich handler (timestamp, colored level, message).

    Call once at application entry (e.g. in main()). Uses stderr so stdout can be
    used for CDK8s YAML output.
    """
    root = logging.getLogger()
    root.setLevel(level)
    # Avoid duplicate handlers when re-running (e.g. tests)
    for h in root.handlers[:]:
        if isinstance(h, RichHandler):
            root.removeHandler(h)
    console = Console(file=sys.stderr, force_terminal=True)
    # markup=False by default so third-party log messages with literal "[]" don't trigger
    # MarkupError. Use extra={"markup": True} on specific log calls that need Rich markup.
    handler = RichHandler(
        console=console,
        show_time=True,
        omit_repeated_times=False,
        show_level=True,
        show_path=show_path,
        log_time_format=log_time_format,
        markup=False,
    )
    handler.setLevel(level)
    root.addHandler(handler)


def get_logger(name: str) -> logging.Logger:
    """Return a logger for the given module name. Use in each module: get_logger(__name__).

    Logging is formatted by Rich (timestamp, colored level, message) when
    configure_logging() has been called at app entry.
    """
    return logging.getLogger(name)
