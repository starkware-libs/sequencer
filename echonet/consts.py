"""
Central configuration for the Echonet components.
"""

from __future__ import annotations

import os
from dataclasses import dataclass
from pathlib import Path
from typing import FrozenSet, Mapping

from types import MappingProxyType


def _csv_lower_set(raw: str) -> FrozenSet[str]:
    """Parse a comma-separated list into a normalized, immutable set."""
    return frozenset(part.strip().lower() for part in str(raw).split(",") if part.strip())


MAX_BLOCK_NUMBER: int = (1 << 200) - 1


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
    """Configuration for talking to the upstream feeder gateway."""

    base_url: str
    headers: Mapping[str, str]
    endpoints: FeederGatewayEndpoints = FeederGatewayEndpoints()


@dataclass(frozen=True, slots=True)
class SequencerGatewayConfig:
    """Configuration for talking to the local sequencer node gateway."""

    base_url_default: str
    endpoints: SequencerGatewayEndpoints = SequencerGatewayEndpoints()


@dataclass(frozen=True, slots=True)
class BlockRangeDefaults:
    """Default block range for streaming/serving blocks."""

    start_block: int
    end_block: int = MAX_BLOCK_NUMBER


@dataclass(frozen=True, slots=True)
class SleepTuning:
    """Runtime tuning for block streaming and special tx pacing."""

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

    alchemy_api_key: str


@dataclass(frozen=True, slots=True)
class EchonetConfig:
    feeder: FeederGatewayConfig
    sequencer: SequencerGatewayConfig
    blocks: BlockRangeDefaults
    sleep: SleepTuning
    paths: PathsConfig
    tx_filter: TxFilterConfig
    resync: ResyncConfig
    l1: L1Config

    @classmethod
    def from_env(cls, env: Mapping[str, str] = os.environ) -> "EchonetConfig":
        feeder_bypass = env.get("FEEDER_X_THROTTLING_BYPASS", "").strip()
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
                start_block=int(env["START_BLOCK_DEFAULT"]),
            ),
            sleep=SleepTuning(),
            paths=PathsConfig(),
            tx_filter=TxFilterConfig(
                blocked_senders=_csv_lower_set(env.get("BLOCKED_SENDERS", "")),
            ),
            resync=ResyncConfig(
                error_threshold=int(env.get("RESYNC_ERROR_THRESHOLD", "1")),
            ),
            l1=L1Config(
                alchemy_api_key=env["L1_ALCHEMY_API_KEY"],
            ),
        )


CONFIG: EchonetConfig = EchonetConfig.from_env()
