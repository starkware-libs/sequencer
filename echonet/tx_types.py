from __future__ import annotations

from enum import Enum


class TxType(str, Enum):
    """
    Starknet transaction types as represented in JSON payloads.
    """

    DECLARE = "DECLARE"
    DEPLOY_ACCOUNT = "DEPLOY_ACCOUNT"
    INVOKE = "INVOKE"
    L1_HANDLER = "L1_HANDLER"
