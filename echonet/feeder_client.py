from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Dict, Mapping, Optional

import requests

from echonet.echonet_types import CONFIG, JsonObject


@dataclass(frozen=True, slots=True)
class FeederClientConfig:
    """Configuration for `FeederClient` (kept as a value object for clarity)."""

    base_url: str = CONFIG.feeder.base_url
    timeout_seconds: float = 20.0


class FeederClient:
    """
    Synchronous client for the Starknet Feeder Gateway.
    """

    def __init__(
        self,
        base_url: str = CONFIG.feeder.base_url,
        headers: Optional[Mapping[str, str]] = None,
        session: Optional[requests.Session] = None,
        request_timeout_seconds: float = 20.0,
    ) -> None:
        self._config = FeederClientConfig(
            base_url=base_url.rstrip("/"),
            timeout_seconds=float(request_timeout_seconds),
        )
        self._headers: Dict[str, str] = (
            dict(headers) if headers is not None else dict(CONFIG.feeder.headers)
        )
        self._session = session or requests.Session()
        self._owns_session = session is None

    def close(self) -> None:
        """Close the underlying session if it was created by this instance."""
        if self._owns_session:
            self._session.close()

    def __enter__(self) -> "FeederClient":
        return self

    def __exit__(self, exc_type, exc, tb) -> None:
        self.close()

    def _get_json(self, endpoint: str, params: Mapping[str, Any]) -> JsonObject:
        resp = self._session.get(
            f"{self._config.base_url}{endpoint}",
            params=dict(params),
            headers=self._headers,
            timeout=self._config.timeout_seconds,
        )
        resp.raise_for_status()
        return resp.json()

    def get_block(
        self,
        block_number: int,
        header_only: Optional[bool] = None,
        with_fee_market_info: Optional[bool] = None,
    ) -> JsonObject:
        params: Dict[str, Any] = {"blockNumber": int(block_number)}
        if header_only:
            params["headerOnly"] = str(bool(header_only)).lower()
        if with_fee_market_info is not None:
            params["withFeeMarketInfo"] = str(bool(with_fee_market_info)).lower()
        return self._get_json(CONFIG.feeder.endpoints.get_block, params=params)

    def get_state_update(self, block_number: int) -> JsonObject:
        return self._get_json(
            CONFIG.feeder.endpoints.get_state_update,
            params={"blockNumber": int(block_number)},
        )

    def get_signature(self, block_number: int | str) -> JsonObject:
        return self._get_json(
            CONFIG.feeder.endpoints.get_signature, params={"blockNumber": block_number}
        )

    def get_transaction(self, transaction_hash: str) -> JsonObject:
        return self._get_json(
            CONFIG.feeder.endpoints.get_transaction,
            params={"transactionHash": str(transaction_hash)},
        )

    def get_class_by_hash(
        self, class_hash: str, block_number: Optional[int | str] = None
    ) -> JsonObject:
        params: Dict[str, Any] = {"classHash": str(class_hash)}
        if block_number:
            params["blockNumber"] = block_number
        return self._get_json(CONFIG.feeder.endpoints.get_class_by_hash, params=params)

    def get_compiled_class_by_class_hash(
        self, class_hash: str, block_number: Optional[int | str] = None
    ) -> JsonObject:
        params: Dict[str, Any] = {"classHash": str(class_hash)}
        if block_number:
            params["blockNumber"] = block_number
        return self._get_json(
            CONFIG.feeder.endpoints.get_compiled_class_by_class_hash, params=params
        )
