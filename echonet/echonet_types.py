from __future__ import annotations

import os
from dataclasses import dataclass
from enum import Enum
from pathlib import Path
from typing import Any, FrozenSet, Mapping, TypeAlias, TypedDict

from types import MappingProxyType

from echonet import helpers
from echonet.constants import (
    ECHONET_ENV_KEYS_PATH,
    ECHONET_ENV_SECRETS_PATH,
    ECHONET_KEYS_FILENAME,
    ECHONET_SECRETS_FILENAME,
    MAX_BLOCK_NUMBER,
)

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


class BlockDumpKind(str, Enum):
    """
    Kinds of payloads available via the debug `/echonet/block_dump` endpoint.
    """

    BLOB = "blob"
    BLOCK = "block"
    STATE_UPDATE = "state_update"


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

    @classmethod
    def from_files(cls, keys_path: Path, secrets_path: Path) -> "EchonetConfig":
        """
        Load config from:
        - keys_path: non-secret parameters persisted on the echonet PVC.
        - secrets_path: secret parameters mounted from a Kubernetes Secret.
        """

        keys = helpers.read_json_object(keys_path)
        secrets = helpers.read_json_object(secrets_path)

        start_block = int(keys["start_block"])
        resync_threshold = int(keys.get("resync_error_threshold", 1))
        blocked_senders_csv = str(keys.get("blocked_senders", ""))

        feeder_bypass = str(secrets.get("feeder_x_throttling_bypass", "")).strip()
        feeder_headers = MappingProxyType(
            {"X-Throttling-Bypass": feeder_bypass} if feeder_bypass else {}
        )
        l1_provider_api_key = str(secrets["l1_provider_api_key"])

        return cls(
            feeder=FeederGatewayConfig(
                base_url=str(
                    keys.get("feeder_base_url", "https://feeder.alpha-mainnet.starknet.io")
                ),
                headers=feeder_headers,
            ),
            sequencer=SequencerGatewayConfig(
                base_url_default=str(
                    keys.get("sequencer_base_url_default", "http://sequencer-node-service:8080")
                ),
            ),
            blocks=BlockRangeDefaults(
                start_block=start_block,
                end_block=int(keys.get("end_block_default", MAX_BLOCK_NUMBER)),
            ),
            sleep=SleepConfig(),
            paths=PathsConfig(),
            tx_filter=TxFilterConfig(
                blocked_senders=helpers.parse_csv_to_lower_set(blocked_senders_csv),
            ),
            resync=ResyncConfig(
                error_threshold=resync_threshold,
            ),
            l1=L1Config(
                l1_provider_api_key=l1_provider_api_key,
            ),
        )


def load_config(
    keys_path: Path = (Path("/data/echonet") / ECHONET_KEYS_FILENAME),
    secrets_path: Path = (Path("/etc/echonet") / ECHONET_SECRETS_FILENAME),
) -> EchonetConfig:
    """
    Load config from file paths.

    Paths can be overridden via environment variables:
    - ECHONET_KEYS_PATH (default: /data/echonet/echonet_keys.json)
    - ECHONET_SECRETS_PATH (default: /etc/echonet/echonet_secrets.json)
    """
    keys_path = Path(os.environ.get(ECHONET_ENV_KEYS_PATH, str(keys_path)))
    secrets_path = Path(os.environ.get(ECHONET_ENV_SECRETS_PATH, str(secrets_path)))

    if not Path(keys_path).exists():
        raise RuntimeError(
            f"Echonet keys file not found: {keys_path}. "
            f"Expected it to exist on the PVC at /data/echonet/{ECHONET_KEYS_FILENAME}."
        )
    if not Path(secrets_path).exists():
        raise RuntimeError(
            f"Echonet secrets file not found: {secrets_path}. "
            f"Expected it to exist at /etc/echonet/{ECHONET_SECRETS_FILENAME}."
        )

    return EchonetConfig.from_files(keys_path=Path(keys_path), secrets_path=Path(secrets_path))


_CONFIG_CACHE: EchonetConfig | None = None


def config() -> EchonetConfig:
    """Load echonet config on first use (lazy)."""
    global _CONFIG_CACHE
    if not _CONFIG_CACHE:
        _CONFIG_CACHE = load_config()
    return _CONFIG_CACHE


class _Config:
    def __getattr__(self, name: str) -> Any:
        return getattr(config(), name)


CONFIG: EchonetConfig = _Config()
