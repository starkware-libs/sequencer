import logging
import sys
from datetime import datetime
from rich.console import Console
from rich.highlighter import NullHighlighter
from rich.theme import Theme


class ProfessionalLogHandler(logging.Handler):
    """Custom log handler that uses Rich Console with no automatic highlighting."""

    def __init__(self, level=logging.NOTSET):
        super().__init__(level)
        # Professional color theme - keep blue for INFO, remove green/purple/magenta
        professional_theme = Theme(
            {
                "logging.level.debug": "dim white",
                "logging.level.info": "blue",  # Keep blue for INFO
                "logging.level.warning": "yellow",
                "logging.level.error": "red",
                "logging.level.critical": "bold red",
            }
        )

        # Create console with NullHighlighter to completely disable automatic highlighting
        self.console = Console(
            theme=professional_theme,
            highlighter=NullHighlighter(),  # Completely disable automatic highlighting
            markup=True,  # Enable markup for explicit formatting
            file=sys.stdout,
            force_terminal=True,
        )

        # Level color mapping
        self.level_colors = {
            logging.DEBUG: "dim white",
            logging.INFO: "blue",
            logging.WARNING: "yellow",
            logging.ERROR: "red",
            logging.CRITICAL: "bold red",
        }

    def emit(self, record):
        try:
            # Get level color
            level_color = self.level_colors.get(record.levelno, "white")
            level_name = record.levelname

            # Format time
            time_str = datetime.fromtimestamp(record.created).strftime("%H:%M:%S")

            # Format message - message already contains Rich markup if provided
            message = record.getMessage()

            # Print with professional formatting: [time] LEVEL message
            # Only color the level, everything else uses explicit markup from message
            self.console.print(
                f"[dim white]{time_str}[/dim white] [{level_color}]{level_name}[/{level_color}] {message}",
                markup=True,
                highlight=False,  # Disable highlighting
            )
        except Exception:
            self.handleError(record)


def get_logger(name: str = __name__, debug: bool = False) -> logging.Logger:
    logger = logging.getLogger(name)
    if logger.hasHandlers():
        logger.handlers.clear()

    handler = ProfessionalLogHandler()
    logger.addHandler(handler)
    logger.setLevel(logging.DEBUG if debug else logging.INFO)
    return logger
