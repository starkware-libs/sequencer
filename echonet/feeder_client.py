from typing import Any, Dict, Optional

import requests

try:
    import aiohttp  # type: ignore
except Exception:  # pragma: no cover - aiohttp may not be present in some runs
    aiohttp = None  # type: ignore

from consts import (
    FEEDER_BASE_URL,
    FEEDER_HEADERS,
    GET_BLOCK_ENDPOINT,
    GET_CLASS_BY_HASH_ENDPOINT,
    GET_SIGNATURE_ENDPOINT,
    GET_STATE_UPDATE_ENDPOINT,
    GET_TRANSACTION_ENDPOINT,
)


class FeederClient:
    """Synchronous client for the Starknet Feeder Gateway"""

    def __init__(
        self,
        base_url: str = FEEDER_BASE_URL,
        headers: Optional[Dict[str, str]] = None,
        session: Optional[requests.Session] = None,
        request_timeout_seconds: float = 20.0,
    ) -> None:
        self.base_url = base_url.rstrip("/")
        self.headers = headers or dict(FEEDER_HEADERS)
        self.session = session or requests.Session()
        self.timeout = request_timeout_seconds

    def get_block(
        self,
        block_number: int,
        *,
        header_only: Optional[bool] = None,
        with_fee_market_info: Optional[bool] = None,
    ) -> Dict[str, Any]:
        params: Dict[str, Any] = {"blockNumber": block_number}
        if header_only is not None:
            params["headerOnly"] = str(header_only).lower()
        if with_fee_market_info is not None:
            params["withFeeMarketInfo"] = str(with_fee_market_info).lower()
        resp = self.session.get(
            f"{self.base_url}{GET_BLOCK_ENDPOINT}",
            params=params,
            headers=self.headers,
            timeout=self.timeout,
        )
        resp.raise_for_status()
        return resp.json()

    def get_state_update(self, block_number: int) -> Dict[str, Any]:
        resp = self.session.get(
            f"{self.base_url}{GET_STATE_UPDATE_ENDPOINT}",
            params={"blockNumber": block_number},
            headers=self.headers,
            timeout=self.timeout,
        )
        resp.raise_for_status()
        return resp.json()

    def get_signature(self, block_number: int | str) -> Dict[str, Any]:
        resp = self.session.get(
            f"{self.base_url}{GET_SIGNATURE_ENDPOINT}",
            params={"blockNumber": block_number},
            headers=self.headers,
            timeout=self.timeout,
        )
        resp.raise_for_status()
        return resp.json()

    def get_transaction(self, transaction_hash: str) -> Dict[str, Any]:
        resp = self.session.get(
            f"{self.base_url}{GET_TRANSACTION_ENDPOINT}",
            params={"transactionHash": transaction_hash},
            headers=self.headers,
            timeout=self.timeout,
        )
        resp.raise_for_status()
        return resp.json()

    def get_class_by_hash(
        self, class_hash: str, *, block_number: Optional[int | str] = None
    ) -> Dict[str, Any]:
        params: Dict[str, Any] = {"classHash": class_hash}
        if block_number is not None:
            params["blockNumber"] = block_number
        resp = self.session.get(
            f"{self.base_url}{GET_CLASS_BY_HASH_ENDPOINT}",
            params=params,
            headers=self.headers,
            timeout=self.timeout,
        )
        resp.raise_for_status()
        return resp.json()
