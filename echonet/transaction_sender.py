from __future__ import annotations

import asyncio
import base64
import gzip
import json
import threading
from dataclasses import dataclass
from typing import Any, ClassVar, Dict, Optional, Sequence, Set

import aiohttp
import requests

from echonet.echonet_types import CONFIG, JsonObject, TxType
from echonet.feeder_client import FeederClient
from echonet.logger import get_logger
from echonet.resync import ResyncExecutor, ResyncPolicy
from echonet.sequencer_manager import SequencerManager
from echonet.shared_context import l1_manager, shared

logger = get_logger("transaction_sender")

_sequencer_manager: Optional[SequencerManager] = None


def _get_sequencer_manager() -> SequencerManager:
    """
    Lazily create the in-cluster sequencer manager.
    """
    global _sequencer_manager
    if _sequencer_manager is None:
        _sequencer_manager = SequencerManager.from_incluster()
    return _sequencer_manager


def _extract_revert_errors_by_tx_hash(block: JsonObject) -> Dict[str, str]:
    """
    Return {tx_hash: revert_error} for any receipt that includes a revert error.

    Feeder blocks contain parallel arrays:
    - transactions[i].transaction_hash
    - transaction_receipts[i].revert_error (optional)
    """
    receipts = block["transaction_receipts"]
    txs = block["transactions"]
    out: Dict[str, str] = {}
    for idx, receipt in enumerate(receipts):
        err = receipt.get("revert_error")
        if err:
            out[txs[idx]["transaction_hash"]] = err
    return out


def _compress_and_encode_json(value: Any) -> str:
    """
    Mirror the Rust `compress_and_encode` helper used by the node:
    JSON -> gzip -> base64.
    """
    json_bytes = json.dumps(value).encode("utf-8")
    compressed = gzip.compress(json_bytes)
    return base64.b64encode(compressed).decode("ascii")


async def fetch_block_transactions(
    feeder: FeederClient,
    block_number: int,
    retries: int,
    retry_backoff_seconds: float,
    max_retry_sleep_seconds: float,
) -> JsonObject:
    """Fetch a feeder block with a retry loop."""
    attempt = 0
    last_err: Optional[Exception] = None

    while attempt <= retries:
        try:
            return feeder.get_block(block_number)
        except Exception as err:
            last_err = err
            if attempt == retries:
                break
            sleep_seconds = min(
                retry_backoff_seconds * (2**attempt),
                max_retry_sleep_seconds,
            )
            logger.warning(
                f"Fetch block {block_number} failed ({attempt + 1}/{retries + 1}): {err}. "
                f"Retrying in {sleep_seconds:.2f}s"
            )
            await asyncio.sleep(sleep_seconds)
            attempt += 1

    assert last_err is not None
    raise last_err


@dataclass(frozen=True, slots=True)
class SenderConfig:
    feeder_url: str = CONFIG.feeder.base_url
    sequencer_url: str = CONFIG.sequencer.base_url_default
    start_block: int = 0
    end_block: Optional[int] = CONFIG.blocks.end_block

    request_timeout_seconds: float = 15.0
    retries: int = 3
    retry_backoff_seconds: float = 0.5
    max_retry_sleep_seconds: float = 30.0

    queue_size: int = 30
    blocks_to_wait_before_failing_tx: int = 50


@dataclass(frozen=True, slots=True)
class TxData:
    tx: JsonObject
    source_block_number: int
    source_timestamp: int


class TxSelector:
    """Pure selection logic: which txs are eligible, and in what order."""

    @staticmethod
    def filter_blocked(
        transactions: Sequence[JsonObject], blocked_senders: Set[str]
    ) -> list[JsonObject]:
        return [tx for tx in transactions if tx.get("sender_address") not in blocked_senders]

    @staticmethod
    def deploy_account_first(transactions: Sequence[JsonObject]) -> list[JsonObject]:
        deploy_txs: list[JsonObject] = []
        other_txs: list[JsonObject] = []

        for tx in transactions:
            (deploy_txs if tx["type"] == TxType.DEPLOY_ACCOUNT else other_txs).append(tx)

        return [*deploy_txs, *other_txs]


class TxTransformer:
    """Prepare transactions for forwarding and update shared/L1 state for special types."""

    def __init__(self, feeder: FeederClient) -> None:
        self._feeder = feeder

    def prepare_for_forwarding(self, tx_data: TxData) -> Optional[JsonObject]:
        tx = tx_data.tx
        tx_type = tx["type"]

        if tx_type == TxType.L1_HANDLER:
            logger.info(
                f"Observed L1_HANDLER tx={tx['transaction_hash']} "
                f"src_bn={tx_data.source_block_number} src_ts={tx_data.source_timestamp}"
            )
            l1_manager.set_new_tx(tx, tx_data.source_timestamp)
            shared.record_sent_tx(tx["transaction_hash"], tx_data.source_block_number)
            return None

        if tx_type == TxType.DECLARE:
            class_hash = tx["class_hash"]
            contract_class = self._feeder.get_class_by_hash(class_hash)
            encoded_program = _compress_and_encode_json(contract_class["sierra_program"])
            contract_class = {
                **contract_class,
                "sierra_program": encoded_program,
                "abi": contract_class["abi"],
            }
            return {**tx, "contract_class": contract_class}

        return tx


class HttpForwarder:
    """Forward prepared txs into the local node and update `shared` bookkeeping."""

    def __init__(self, session: aiohttp.ClientSession, sequencer_url: str) -> None:
        self._session = session
        self._sequencer_url = sequencer_url.rstrip("/")

    async def _post_tx_timestamp(self, tx_hash: str, source_timestamp: int) -> None:
        url = f"{self._sequencer_url}{CONFIG.sequencer.endpoints.update_timestamps}"
        headers = {"Content-Type": "application/json"}
        payload = {tx_hash: int(source_timestamp)}
        try:
            async with self._session.post(url, json=payload, headers=headers) as response:
                if response.status != requests.codes.ok:
                    text = await response.text()
                    logger.warning(
                        f"Failed to post tx timestamp (status={response.status}) tx={tx_hash}: {text}"
                    )
        except Exception as err:
            logger.warning(f"Failed to post tx timestamp tx={tx_hash}: {err}")

    async def forward(
        self, tx: JsonObject, source_block_number: int, source_timestamp: int
    ) -> None:
        url = f"{self._sequencer_url}{CONFIG.sequencer.endpoints.add_transaction}"
        headers = {"Content-Type": "application/json"}
        tx_hash = tx["transaction_hash"]

        await self._post_tx_timestamp(tx_hash, source_timestamp)

        async with self._session.post(url, json=tx, headers=headers) as response:
            text = await response.text()
            if response.status != requests.codes.ok:
                logger.warning(f"Forward failed ({response.status}): {text}")
                shared.record_gateway_error(
                    tx_hash, response.status, text, block_number=source_block_number
                )
            else:
                logger.info(f"Forwarded tx: {tx_hash}")
                shared.record_sent_tx(tx_hash, source_block_number)

        if tx["type"] == TxType.DEPLOY_ACCOUNT:
            await asyncio.sleep(CONFIG.sleep.deploy_account_sleep_time_seconds)


class TransactionSenderService:
    """
    Stream blocks from the feeder gateway and forward transactions into the local node.
    """

    def __init__(self) -> None:
        self._stop_event = threading.Event()

    def stop(self) -> None:
        self._stop_event.set()

    async def run(self, config: SenderConfig) -> None:
        timeout = aiohttp.ClientTimeout(total=config.request_timeout_seconds)
        feeder = FeederClient(base_url=config.feeder_url)

        async with aiohttp.ClientSession(timeout=timeout) as session:
            tx_queue: "asyncio.Queue[Optional[TxData]]" = asyncio.Queue(maxsize=config.queue_size)

            transformer = TxTransformer(feeder)
            forwarder = HttpForwarder(session, sequencer_url=config.sequencer_url)
            resync_policy = ResyncPolicy(
                blocks_to_wait_before_failing_tx=config.blocks_to_wait_before_failing_tx
            )
            resync_executor = ResyncExecutor(get_sequencer_manager=_get_sequencer_manager)

            async def drain_queue() -> None:
                while True:
                    try:
                        item = tx_queue.get_nowait()
                    except asyncio.QueueEmpty:
                        return
                    else:
                        tx_queue.task_done()
                        if item is None:
                            return

            # TODO(Ron): shorten this function
            async def producer() -> None:
                await asyncio.sleep(CONFIG.sleep.producer_startup_sleep_seconds)
                block_number = config.start_block
                while not self._stop_event.is_set():
                    if config.end_block and block_number > config.end_block:
                        return

                    shared.set_sender_current_block(block_number)

                    block = await fetch_block_transactions(
                        feeder,
                        block_number,
                        retries=config.retries,
                        retry_backoff_seconds=config.retry_backoff_seconds,
                        max_retry_sleep_seconds=config.max_retry_sleep_seconds,
                    )

                    timestamp = block["timestamp"]
                    shared.store_fgw_block(block_number, block)

                    revert_errors = _extract_revert_errors_by_tx_hash(block)
                    if revert_errors:
                        shared.record_mainnet_revert_errors(block_number, revert_errors)

                    all_txs = block["transactions"]
                    valid_txs = TxSelector.filter_blocked(all_txs, CONFIG.tx_filter.blocked_senders)
                    ordered_txs = TxSelector.deploy_account_first(valid_txs)
                    logger.info(
                        f"Block {block_number}: total={len(all_txs)} valid={len(ordered_txs)}"
                    )

                    for tx in ordered_txs:
                        tx_data = TxData(
                            tx=tx,
                            source_block_number=block_number,
                            source_timestamp=timestamp,
                        )
                        await tx_queue.put(tx_data)

                    if ordered_txs:
                        shared.record_forwarded_block(block_number, len(ordered_txs))
                    shared.set_block_timestamp(timestamp)

                    gw_errors, sent_tx_hashes = shared.get_resync_evaluation_inputs()
                    trigger = resync_policy.evaluate(
                        gateway_errors=gw_errors,
                        sent_tx_hashes=sent_tx_hashes,
                        current_block=block_number,
                    )
                    if trigger:
                        logger.warning(
                            f"Resync triggered by tx {trigger['tx_hash']} at block {trigger['block_number']}: "
                            f"{trigger['reason']}"
                        )
                        await drain_queue()
                        block_number = await resync_executor.execute(trigger=trigger)
                        continue

                    block_number += 1

            async def consumer() -> None:
                while True:
                    item = await tx_queue.get()
                    try:
                        if item is None:
                            return

                        prepared = transformer.prepare_for_forwarding(item)
                        if prepared is None:  # L1_HANDLER
                            while shared.is_pending_tx(item.tx["transaction_hash"]):
                                await asyncio.sleep(CONFIG.tx_sender.poll_interval_seconds)
                            continue

                        while (
                            shared.get_pending_tx_count()
                            >= CONFIG.tx_sender.max_pending_txs_before_pausing
                        ):
                            await asyncio.sleep(CONFIG.tx_sender.poll_interval_seconds)

                        await forwarder.forward(
                            prepared,
                            source_block_number=item.source_block_number,
                            source_timestamp=item.source_timestamp,
                        )
                    finally:
                        tx_queue.task_done()

            producer_task = asyncio.create_task(producer())
            consumer_task = asyncio.create_task(consumer())

            await producer_task
            await tx_queue.put(None)
            await tx_queue.join()
            await consumer_task


class TransactionSenderRunner:
    """Run `TransactionSenderService` in a dedicated thread."""

    _background_instance: ClassVar[Optional["TransactionSenderRunner"]] = None
    _background_lock: ClassVar[threading.Lock] = threading.Lock()

    def __init__(self) -> None:
        self._lock = threading.Lock()
        self._thread: Optional[threading.Thread] = None
        self._starting = False
        self._service = TransactionSenderService()

    @classmethod
    def background(cls) -> "TransactionSenderRunner":
        """Return the process-wide background runner instance."""
        with cls._background_lock:
            if cls._background_instance is None:
                cls._background_instance = cls()
            return cls._background_instance

    def start(self, config: SenderConfig) -> bool:
        old_thread: Optional[threading.Thread] = None
        # `Thread.is_alive()` alone is not enough: there is a tiny window where a thread
        # object exists but hasn't started running yet. Guard with a lock + flag so
        # concurrent callers can't start multiple background threads.
        with self._lock:
            if self._starting:
                return False
            if self._thread:
                if self._thread.is_alive():
                    return False
                # The previous thread finished. Join it before replacing the thread object.
                old_thread = self._thread
                self._thread = None
            self._starting = True

        if old_thread:
            old_thread.join()

        def _runner() -> None:
            try:
                _get_sequencer_manager().initial_revert_then_restore(
                    block_number=config.start_block
                )

                shared.set_initial_start_block_if_absent(config.start_block)
                shared.set_current_start_block(config.start_block)

                logger.info(
                    f"TransactionSenderRunner starting: feeder_url={config.feeder_url} "
                    f"sequencer_url={config.sequencer_url} start_block={config.start_block} "
                    f"end_block={config.end_block} timeout={config.request_timeout_seconds} "
                    f"retries={config.retries} backoff={config.retry_backoff_seconds}"
                )
                asyncio.run(self._service.run(config))
            finally:
                logger.info("TransactionSenderRunner stopped")

        try:
            t = threading.Thread(target=_runner, name="TransactionSenderRunner", daemon=True)
            with self._lock:
                self._thread = t
            t.start()
            return True
        finally:
            with self._lock:
                self._starting = False

    def stop(self) -> bool:
        """
        Signal the background sender thread to stop.
        """
        self._service.stop()

        t: Optional[threading.Thread]
        with self._lock:
            t = self._thread

        if t is None:
            return True

        t.join()

        return not t.is_alive()


def start_background_sender(
    feeder_url: str = CONFIG.feeder.base_url,
    sequencer_url: str = CONFIG.sequencer.base_url_default,
    start_block: int = CONFIG.blocks.start_block,
    end_block: int = CONFIG.blocks.end_block,
    request_timeout_seconds: float = 15.0,
    retries: int = 3,
    retry_backoff_seconds: float = 0.5,
) -> bool:
    """
    Start the background transaction sender.
    """
    cfg = SenderConfig(
        feeder_url=feeder_url,
        sequencer_url=sequencer_url,
        start_block=start_block,
        end_block=end_block,
        request_timeout_seconds=request_timeout_seconds,
        retries=retries,
        retry_backoff_seconds=retry_backoff_seconds,
    )
    return TransactionSenderRunner.background().start(config=cfg)


def stop_background_sender() -> bool:
    """
    Stop the background transaction sender.
    """
    return TransactionSenderRunner.background().stop()
