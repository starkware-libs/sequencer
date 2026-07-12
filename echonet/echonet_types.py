from __future__ import annotations

import os
from dataclasses import dataclass
from enum import Enum
from pathlib import Path
from types import MappingProxyType
from typing import Any, FrozenSet, Mapping, TypeAlias, TypedDict

from echonet import helpers
from echonet.constants import (
    ECHONET_ENV_KEYS_PATH,
    ECHONET_ENV_SECRETS_PATH,
    ECHONET_KEYS_FILENAME,
    ECHONET_SECRETS_FILENAME,
    MAX_BLOCK_NUMBER,
)

JsonObject: TypeAlias = dict[str, Any]


@dataclass(frozen=True, slots=True)
class SeverityConfig:
    """Tuning parameters for report/UI severity classification."""

    bad_count_threshold: int = 11


@dataclass(frozen=True)
class BlockStoreTuning:
    """
    Tuning parameters for SharedContext's in-memory block retention.
    """

    max_blocks_to_keep_in_memory: int = 100
    # Blob bodies dominate memory and are rarely read back; keep a much tighter
    # window than block docs — older bodies are archived to disk.
    max_blob_bodies_to_keep_in_memory: int = 30


class ResyncTriggerPayload(TypedDict):
    """
    Metadata stored when a transaction causes a resync trigger.

    This is  a subset of `ResyncTrigger` (which is persisted and
    includes a `count` field).
    """

    tx_hash: str
    failure_block_number: int
    revert_target_block_number: int
    reason: str


class ResyncTrigger(ResyncTriggerPayload):
    """
    Metadata stored when a transaction causes a resync trigger.

    This is persisted into report snapshots and rendered by the reporting UI/text endpoints.
    """

    count: int


ResyncTriggerMap: TypeAlias = dict[str, ResyncTrigger]  # tx_hash -> metadata


class RevertErrorInfo(TypedDict):
    block_number: int
    error: str


def create_revert_error_info(block_number: int, error: str) -> RevertErrorInfo:
    return {"block_number": int(block_number), "error": error}


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
class ResyncConfig:
    """Behavior of the resync revert-and-restore flow."""

    # When True, the revert step rewinds only to `max(start_block, next_start_block -
    # revert_lookback_blocks)` instead of all the way back to the previous start block,
    # bounding how many blocks must be re-synced from the feeder gateway.
    bounded_revert_enabled: bool = False
    revert_lookback_blocks: int = 100


@dataclass(frozen=True, slots=True)
class SleepConfig:
    """Sleep/delay settings for block streaming and special transaction pacing."""

    producer_startup_sleep_seconds: float = 10.0
    gateway_error_retry_interval_seconds: float = 10.0


@dataclass(frozen=True, slots=True)
class TxSenderTuning:
    """Tuning parameters for transaction_sender consumer."""

    max_pending_txs_before_pausing: int = 15
    poll_interval_seconds: float = 0.25


@dataclass(frozen=True, slots=True)
class PathsConfig:
    """Filesystem locations for auxiliary artifacts (reports, snapshots, etc.)."""

    log_dir: Path = Path("/data/echonet")
    block_hash_cli_path: Path = Path("/data/echonet/bin/starknet_committer_and_os_cli")


@dataclass(frozen=True, slots=True)
class OsRunnerConfig:
    """
    Configuration for running the Starknet OS over each received blob, via the
    block-hash CLI binary's `os run-os-stateless` subcommand (which must be
    built with the `os_input` and `transaction_serde` features).
    """

    enabled: bool = True
    layout: str = "all_cairo"
    cli_timeout_secs: int = 600
    chain_id: str = "SN_MAIN"
    strk_fee_token_address: str = (
        "0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"
    )
    # Bounded backlog of OS-run tasks; tasks beyond it are dropped (logged) so the
    # Flask handler never blocks blob storage.
    queue_size: int = 30
    # Worker threads draining the queue, one OS CLI subprocess each. At ~5 runs/min
    # per worker vs ~26 mainnet blocks/min, ≥6 are needed; 8 also uses the pod's
    # cpu-limit headroom under burst while staying within the memory limit.
    parallelism: int = 8
    # Width of the blob's `recent_state_commitment_infos` vector
    # (N_BLOCK_HASHES_BACK_IN_BLOB): a block's commit info is only reachable from
    # blobs at most this many heights newer.
    recent_state_commitment_window: int = 10
    # Most-recent failed-OS-run input dumps to keep on the PVC for debugging.
    max_failed_dumps: int = 10


@dataclass(frozen=True, slots=True)
class TxFilterConfig:
    """Transaction forwarding filter parameters."""

    blocked_senders: FrozenSet[str]


@dataclass(frozen=True, slots=True)
class L1Config:
    """External provider credentials for L1 access."""

    l1_events_provider_api_key: str


@dataclass(frozen=True, slots=True)
class GcpLogsConfig:
    """Config used to build Google Cloud Logs Explorer links in the report UI."""

    project_id: str
    location: str
    gke_cluster_name: str


@dataclass(frozen=True, slots=True)
class NotificationsConfig:
    """Outbound alerting. Empty `slack_webhook_url` disables Slack notifications."""

    slack_webhook_url: str


@dataclass(frozen=True, slots=True)
class EchonetConfig:
    feeder: FeederGatewayConfig
    sequencer: SequencerGatewayConfig
    blocks: BlockRangeDefaults
    resync: ResyncConfig
    sleep: SleepConfig
    tx_sender: TxSenderTuning
    block_store: BlockStoreTuning
    severity: SeverityConfig
    paths: PathsConfig
    os_runner: OsRunnerConfig
    tx_filter: TxFilterConfig
    l1: L1Config
    gcp_logs: GcpLogsConfig
    notifications: NotificationsConfig

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
        blocked_senders_csv = str(keys.get("blocked_senders", ""))
        max_pending_txs_before_pausing = int(keys.get("max_pending_txs_before_pausing", 15))

        feeder_bypass = str(secrets.get("feeder_x_throttling_bypass", "")).strip()
        feeder_headers = MappingProxyType(
            {"X-Throttling-Bypass": feeder_bypass} if feeder_bypass else {}
        )
        l1_events_provider_api_key = str(secrets["l1_events_provider_api_key"])
        gcp_project_id = str(secrets.get("gcp_project_id", ""))
        gcp_location = str(secrets.get("gcp_location", ""))
        gke_cluster_name = str(secrets.get("gke_cluster_name", ""))

        slack_webhook_url = str(secrets.get("slack_webhook_url", "")).strip()

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
            resync=ResyncConfig(
                bounded_revert_enabled=bool(keys.get("resync_bounded_revert_enabled", False)),
                revert_lookback_blocks=int(keys.get("resync_revert_lookback_blocks", 100)),
            ),
            sleep=SleepConfig(),
            tx_sender=TxSenderTuning(
                max_pending_txs_before_pausing=max_pending_txs_before_pausing,
            ),
            block_store=BlockStoreTuning(
                max_blocks_to_keep_in_memory=max(
                    BlockStoreTuning.max_blocks_to_keep_in_memory,
                    max_pending_txs_before_pausing // 2,
                ),
            ),
            severity=SeverityConfig(),
            paths=PathsConfig(),
            os_runner=OsRunnerConfig(),
            tx_filter=TxFilterConfig(
                blocked_senders=helpers.parse_csv_to_lower_set(blocked_senders_csv),
            ),
            l1=L1Config(
                l1_events_provider_api_key=l1_events_provider_api_key,
            ),
            gcp_logs=GcpLogsConfig(
                project_id=gcp_project_id,
                location=gcp_location,
                gke_cluster_name=gke_cluster_name,
            ),
            notifications=NotificationsConfig(
                slack_webhook_url=slack_webhook_url,
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
