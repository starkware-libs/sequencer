from __future__ import annotations

import requests

from echonet.echonet_types import CONFIG
from echonet.logger import get_logger

logger = get_logger("notifier")

_SLACK_POST_TIMEOUT_SECONDS = 10


def notify_resync(
    *,
    reason: str,
    tx_hash: str,
    failure_block_number: int,
    next_start_block: int,
    is_repeated_trigger: bool,
) -> None:
    """Send a best-effort Slack alert that a resync was triggered.

    No-op when no Slack webhook is configured. Never raises: a notification
    failure must not interrupt the resync itself, so all errors are logged
    and swallowed.
    """
    webhook_url = CONFIG.notifications.slack_webhook_url
    if not webhook_url:
        logger.warning("Slack resync notification dropped: no webhook configured")
        return

    payload = _build_slack_payload(
        reason=reason,
        tx_hash=tx_hash,
        failure_block_number=failure_block_number,
        next_start_block=next_start_block,
        is_repeated_trigger=is_repeated_trigger,
    )
    try:
        response = requests.post(webhook_url, json=payload, timeout=_SLACK_POST_TIMEOUT_SECONDS)
        if response.status_code != requests.codes.ok:
            logger.warning(
                f"Slack resync notification failed: status={response.status_code} "
                f"body={response.text!r}"
            )
    except requests.RequestException as err:
        logger.warning(f"Slack resync notification failed: {err}")


def _build_slack_payload(
    *,
    reason: str,
    tx_hash: str,
    failure_block_number: int,
    next_start_block: int,
    is_repeated_trigger: bool,
) -> dict:
    repeat_marker = (
        " (repeated trigger — previous resync did not clear it)" if is_repeated_trigger else ""
    )
    header = f":rotating_light: Echonet resync triggered{repeat_marker}"

    fields = [
        f"*Reason:*\n{reason}",
        f"*Trigger tx:*\n`{tx_hash}`",
        f"*Failure block:*\n{failure_block_number}",
        f"*New start block:*\n{next_start_block}",
    ]

    blocks: list[dict] = [
        {"type": "header", "text": {"type": "plain_text", "text": header}},
        {
            "type": "section",
            "fields": [{"type": "mrkdwn", "text": field} for field in fields],
        },
    ]

    return {"text": header, "blocks": blocks}
