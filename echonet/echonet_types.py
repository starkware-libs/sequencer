from __future__ import annotations

import os
from dataclasses import dataclass
from enum import Enum
from pathlib import Path
from typing import Any, FrozenSet, Mapping, TypeAlias, TypedDict

from types import MappingProxyType

from echonet import helpers
from echonet.constants import MAX_BLOCK_NUMBER

JsonObject: TypeAlias = dict[str, Any]


class ResyncTriggerPayload(TypedDict):
    """
    Metadata stored when a transaction causes a resync trigger.

    This is  a subset of `ResyncTrigger` (which is persisted and
    includes a `count` field).
    """

    tx_hash: str
    block_number: int
    reason: str


class ResyncTrigger(ResyncTriggerPayload):
    """
    Metadata stored when a transaction causes a resync trigger.

    This is persisted into report snapshots and rendered by `reports.py`.
    """

    count: int


ResyncTriggerMap: TypeAlias = dict[str, ResyncTrigger]  # tx_hash -> metadata


class TxType(str, Enum):
    """
    Starknet transaction types as represented in JSON payloads.
    """

    DECLARE = "DECLARE"
    DEPLOY_ACCOUNT = "DEPLOY_ACCOUNT"
    INVOKE = "INVOKE"
    L1_HANDLER = "L1_HANDLER"


@dataclass(frozen=True, slots=True)
class FeederGatewayEndpoints:
    """Feeder Gateway endpoint paths (relative to the gateway base URL)."""

    get_block: str = "/feeder_gateway/get_block"
    get_state_update: str = "/feeder_gateway/get_state_update"
    get_signature: str = "/feeder_gateway/get_signature"
    get_transaction: str = "/feeder_gateway/get_transaction"
    get_class_by_hash: str = "/feeder_gateway/get_class_by_hash"
    get_compiled_class_by_class_hash: str = "/feeder_gateway/get_compiled_class_by_class_hash"


@dataclass(frozen=True, slots=True)
class SequencerGatewayEndpoints:
    """Sequencer gateway endpoint paths (relative to the gateway base URL)."""

    add_transaction: str = "/gateway/add_transaction"


@dataclass(frozen=True, slots=True)
class FeederGatewayConfig:
    """Configuration for connecting to the upstream feeder gateway."""

    base_url: str
    headers: Mapping[str, str]
    endpoints: FeederGatewayEndpoints = FeederGatewayEndpoints()


@dataclass(frozen=True, slots=True)
class SequencerGatewayConfig:
    """Configuration for connecting to the local sequencer node gateway."""

    base_url_default: str
    endpoints: SequencerGatewayEndpoints = SequencerGatewayEndpoints()


@dataclass(frozen=True, slots=True)
class BlockRangeDefaults:
    """Default block range for streaming/serving blocks."""

    start_block: int
    end_block: int = MAX_BLOCK_NUMBER


@dataclass(frozen=True, slots=True)
class SleepConfig:
    """Sleep/delay settings for block streaming and special transaction pacing."""

    sleep_between_blocks_seconds: float = 2.0
    initial_slow_blocks_count: int = 10
    extra_sleep_time_seconds: float = 3.0
    deploy_account_sleep_time_seconds: float = 2.0


@dataclass(frozen=True, slots=True)
class PathsConfig:
    """Filesystem locations for auxiliary artifacts (reports, snapshots, etc.)."""

    log_dir: Path = Path("/data/echonet")


@dataclass(frozen=True, slots=True)
class TxFilterConfig:
    """Transaction forwarding filter parameters."""

    blocked_senders: FrozenSet[str]


@dataclass(frozen=True, slots=True)
class ResyncConfig:
    """Thresholds for resync decisions."""

    error_threshold: int = 1


@dataclass(frozen=True, slots=True)
class L1Config:
    """External provider credentials for L1 access."""

    l1_provider_api_key: str


@dataclass(frozen=True, slots=True)
class EchonetConfig:
    feeder: FeederGatewayConfig
    sequencer: SequencerGatewayConfig
    blocks: BlockRangeDefaults
    sleep: SleepConfig
    paths: PathsConfig
    tx_filter: TxFilterConfig
    resync: ResyncConfig
    l1: L1Config

    @staticmethod
    def _load_keys_from_dir(keys_dir: str) -> dict[str, str]:
        keys_path = Path(keys_dir)

        if not keys_path.exists():
            raise FileNotFoundError(f"Keys directory does not exist: {keys_dir}. ")

        if not keys_path.is_dir():
            raise ValueError(f"Keys path exists but is not a directory: {keys_dir}")

        keys: dict[str, str] = {}

        required_keys = ["L1_PROVIDER_API_KEY", "START_BLOCK_DEFAULT"]
        optional_keys = {
            "FEEDER_X_THROTTLING_BYPASS": "",
            "BLOCKED_SENDERS": "",
            "RESYNC_ERROR_THRESHOLD": "1",
        }

        for key_name in required_keys:
            key_file = keys_path / key_name
            if not key_file.exists():
                raise FileNotFoundError(f"Required key not found: {key_file}. ")
            with open(key_file, "r", encoding="utf-8") as f:
                keys[key_name] = f.read().strip()

        for key_name, default in optional_keys.items():
            key_file = keys_path / key_name
            if key_file.exists():
                with open(key_file, "r", encoding="utf-8") as f:
                    keys[key_name] = f.read().strip()
            else:
                keys[key_name] = default

        return keys

    @classmethod
    def from_env(cls, env: Mapping[str, str] = os.environ) -> "EchonetConfig":
        keys_dir = env.get("KEYS_DIR")
        if not keys_dir:
            raise ValueError("KEYS_DIR environment variable must be set. ")
        keys = cls._load_keys_from_dir(keys_dir)

        feeder_bypass = keys["FEEDER_X_THROTTLING_BYPASS"]
        feeder_headers = (
            MappingProxyType({"X-Throttling-Bypass": feeder_bypass})
            if feeder_bypass
            else MappingProxyType({})
        )

        return cls(
            feeder=FeederGatewayConfig(
                base_url="https://feeder.alpha-mainnet.starknet.io",
                headers=feeder_headers,
            ),
            sequencer=SequencerGatewayConfig(
                base_url_default="http://sequencer-node-service:8080",
            ),
            blocks=BlockRangeDefaults(
                start_block=int(keys["START_BLOCK_DEFAULT"]),
            ),
            sleep=SleepConfig(),
            paths=PathsConfig(),
            tx_filter=TxFilterConfig(
                blocked_senders=helpers.parse_csv_to_lower_set(keys["BLOCKED_SENDERS"]),
            ),
            resync=ResyncConfig(
                error_threshold=int(keys["RESYNC_ERROR_THRESHOLD"]),
            ),
            l1=L1Config(
                l1_provider_api_key=keys["L1_PROVIDER_API_KEY"],
            ),
        )


CONFIG: EchonetConfig = EchonetConfig.from_env()
