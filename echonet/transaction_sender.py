from __future__ import annotations

import asyncio
import json
from dataclasses import dataclass
from typing import Any, ClassVar, Dict, Mapping, Optional, Sequence, Set

import aiohttp
import base64
import consts
import gzip
import reports
import requests
import threading
from feeder_client import FeederClient
from logger import get_logger
from manage_sequencer import initial_revert_then_restore, resync_sequencer, scale_sequencer_to_zero
from shared_context import l1_manager, shared
from tx_types import TxType

logger = get_logger("transaction_sender")

JsonObject = Dict[str, Any]


def _extract_revert_errors_by_tx_hash(block: Mapping[str, Any]) -> Dict[str, str]:
    """
    Return {tx_hash: revert_error} for any receipt that includes a revert error.

    Feeder blocks contain parallel arrays:
    - transactions[i].transaction_hash
    - transaction_receipts[i].revert_error (optional)
    """
    receipts = block.get("transaction_receipts")
    txs = block.get("transactions")
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
            sleep_seconds = retry_backoff_seconds * (2**attempt)
            logger.warning(
                "Fetch block %s failed (%s/%s): %s. Retrying in %.2fs",
                block_number,
                attempt + 1,
                retries + 1,
                err,
                sleep_seconds,
            )
            await asyncio.sleep(sleep_seconds)
            attempt += 1

    assert last_err is not None
    raise last_err


@dataclass(frozen=True, slots=True)
class SenderConfig:
    feeder_url: str = consts.CONFIG.feeder.base_url
    sequencer_url: str = consts.CONFIG.sequencer.base_url_default
    start_block: int = 0
    end_block: Optional[int] = consts.CONFIG.blocks.end_block

    request_timeout_seconds: float = 15.0
    retries: int = 3
    retry_backoff_seconds: float = 0.5
    sleep_between_blocks_seconds: float = consts.CONFIG.sleep.sleep_between_blocks_seconds

    queue_size: int = 100
    resync_min_age_blocks: int = 30


@dataclass(frozen=True, slots=True)
class TxEnvelope:
    tx: JsonObject
    source_block_number: int
    source_timestamp: Optional[int]


@dataclass(frozen=True, slots=True)
class ResyncTrigger:
    tx_hash: str
    block_number: int
    reason: str


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
            (deploy_txs if tx.get("type") == TxType.DEPLOY_ACCOUNT else other_txs).append(tx)

        return [*deploy_txs, *other_txs]


class TxTransformer:
    """Prepare transactions for forwarding and update shared/L1 state for special types."""

    def __init__(self, feeder: FeederClient) -> None:
        self._feeder = feeder

    def prepare_for_forwarding(self, env: TxEnvelope) -> Optional[JsonObject]:
        tx = env.tx
        tx_type = tx.get("type")

        if tx_type == TxType.L1_HANDLER:
            logger.info(
                "Observed L1_HANDLER tx=%s src_bn=%s src_ts=%s",
                tx.get("transaction_hash"),
                env.source_block_number,
                env.source_timestamp,
            )
            l1_manager.set_new_tx(tx, env.source_timestamp)
            shared.record_sent_tx(tx["transaction_hash"], env.source_block_number)
            return None

        if tx_type != TxType.DECLARE:
            return tx

        class_hash = tx.get("class_hash")
        contract_class = self._feeder.get_class_by_hash(class_hash)
        encoded_program = _compress_and_encode_json(contract_class.get("sierra_program"))
        contract_class = {
            **contract_class,
            "sierra_program": encoded_program,
            "abi": contract_class.get("abi"),
        }
        return {**tx, "contract_class": contract_class}


class HttpForwarder:
    """Forward prepared txs into the local node and update `shared` bookkeeping."""

    def __init__(self, session: aiohttp.ClientSession, *, sequencer_url: str) -> None:
        self._session = session
        self._sequencer_url = sequencer_url.rstrip("/")

    async def forward(self, tx: JsonObject, *, source_block_number: int) -> None:
        url = f"{self._sequencer_url}{consts.CONFIG.sequencer.endpoints.add_transaction}"
        headers = {"Content-Type": "application/json"}

        async with self._session.post(url, json=tx, headers=headers) as response:
            text = await response.text()
            tx_hash = tx["transaction_hash"]
            if response.status != requests.codes.ok:
                logger.warning("Forward failed (%s): %s", response.status, text)
                shared.record_gateway_error(
                    tx_hash, response.status, text, block_number=source_block_number
                )
            else:
                logger.info("Forwarded tx: %s", tx_hash)
                shared.record_sent_tx(tx_hash, source_block_number)

        if tx["type"] == TxType.DEPLOY_ACCOUNT:
            await asyncio.sleep(consts.CONFIG.sleep.deploy_account_sleep_time_seconds)


class ResyncPolicy:
    """Decide whether the system has accumulated enough evidence to resync."""

    def __init__(self, *, min_age_blocks: int) -> None:
        self._min_age_blocks = min_age_blocks

    def evaluate(
        self,
        *,
        gateway_errors: Dict[str, JsonObject],
        sent_tx_hashes: Dict[str, int],
        current_block: int,
    ) -> Optional[ResyncTrigger]:
        gw = gateway_errors
        sent_map = sent_tx_hashes
        threshold_block = current_block - self._min_age_blocks

        candidates: list[tuple[str, int, str]] = []

        for txh, info in gw.items():
            bn = info["block_number"]
            if bn <= threshold_block:
                candidates.append((txh, bn, f"Gateway error: {info['response']}"))

        for txh, bn in sent_map.items():
            if bn <= threshold_block:
                candidates.append(
                    (txh, bn, f"Still pending after >= {self._min_age_blocks} blocks")
                )

        if len(candidates) < consts.CONFIG.resync.error_threshold:
            return None

        txh_first, bn_first, reason_first = min(candidates, key=lambda item: item[1])
        return ResyncTrigger(
            tx_hash=txh_first,
            block_number=bn_first,
            reason=f"Resync after >= {consts.CONFIG.resync.error_threshold} errors: {reason_first}",
        )


class ResyncExecutor:
    """Run the resync flow and update shared/global start-block state."""

    async def execute(self, *, trigger: ResyncTrigger) -> int:
        is_repeat = shared.record_resync_cause(
            trigger.tx_hash, trigger.block_number, trigger.reason
        )
        next_start_block = trigger.block_number + 1 if is_repeat else trigger.block_number

        scale_sequencer_to_zero()
        reports.write_pre_resync_reports(
            trigger_tx_hash=trigger.tx_hash,
            trigger_block=trigger.block_number,
            trigger_reason=trigger.reason,
            snapshot=shared.get_report_snapshot(),
            logger=logger,
        )
        shared.clear_for_resync()

        loop = asyncio.get_running_loop()
        await loop.run_in_executor(None, resync_sequencer, next_start_block)

        shared.set_current_start_block(next_start_block)
        return next_start_block


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
            tx_queue: "asyncio.Queue[Optional[TxEnvelope]]" = asyncio.Queue(
                maxsize=config.queue_size
            )

            transformer = TxTransformer(feeder)
            forwarder = HttpForwarder(session, sequencer_url=config.sequencer_url)
            resync_policy = ResyncPolicy(min_age_blocks=config.resync_min_age_blocks)
            resync_executor = ResyncExecutor()

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

            async def producer() -> None:
                block_number = config.start_block
                current_start_block = config.start_block
                while not self._stop_event.is_set():
                    if config.end_block is not None and block_number > config.end_block:
                        return

                    shared.set_sender_current_block(block_number)

                    block = await fetch_block_transactions(
                        feeder,
                        block_number,
                        retries=config.retries,
                        retry_backoff_seconds=config.retry_backoff_seconds,
                    )

                    timestamp = block["timestamp"]
                    shared.store_fgw_block(block_number, block)

                    revert_errors = _extract_revert_errors_by_tx_hash(block)
                    if revert_errors:
                        shared.add_mainnet_revert_errors(revert_errors)

                    all_txs = block["transactions"]
                    valid_txs = TxSelector.filter_blocked(
                        all_txs, consts.CONFIG.tx_filter.blocked_senders
                    )
                    ordered_txs = TxSelector.deploy_account_first(valid_txs)
                    logger.info(
                        "Block %s: total=%s valid=%s",
                        block_number,
                        len(all_txs),
                        len(ordered_txs),
                    )

                    for tx in ordered_txs:
                        env = TxEnvelope(
                            tx=tx,
                            source_block_number=block_number,
                            source_timestamp=timestamp,
                        )
                        await tx_queue.put(env)

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
                            "Resync triggered by tx %s at block %s: %s",
                            trigger.tx_hash,
                            trigger.block_number,
                            trigger.reason,
                        )
                        await drain_queue()
                        block_number = await resync_executor.execute(trigger=trigger)
                        current_start_block = block_number
                        continue

                    block_number += 1
                    await self._sleep_between_blocks(
                        current_block=block_number,
                        start_block=current_start_block,
                        base_sleep_seconds=config.sleep_between_blocks_seconds,
                    )

            async def consumer() -> None:
                while True:
                    item = await tx_queue.get()
                    try:
                        if item is None:
                            return

                        prepared = transformer.prepare_for_forwarding(item)
                        if prepared is None:
                            continue

                        await forwarder.forward(
                            prepared, source_block_number=item.source_block_number
                        )
                    finally:
                        tx_queue.task_done()

            producer_task = asyncio.create_task(producer())
            consumer_task = asyncio.create_task(consumer())

            await producer_task
            await tx_queue.put(None)
            await tx_queue.join()
            await consumer_task

    async def _sleep_between_blocks(
        self,
        *,
        current_block: int,
        start_block: int,
        base_sleep_seconds: float,
    ) -> None:
        effective_sleep = base_sleep_seconds
        if (current_block - start_block) < consts.CONFIG.sleep.initial_slow_blocks_count:
            effective_sleep = (
                float(base_sleep_seconds) + consts.CONFIG.sleep.extra_sleep_time_seconds
            )
        if effective_sleep > 0:
            await asyncio.sleep(effective_sleep)


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

    def start(self, *, config: SenderConfig) -> bool:
        # `Thread.is_alive()` alone is not enough: there is a tiny window where a thread
        # object exists but hasn't started running yet. Guard with a lock + flag so
        # concurrent callers can't start multiple background threads.
        with self._lock:
            if self._starting:
                return False
            if self._thread and self._thread.is_alive():
                return False
            self._starting = True

        def _runner() -> None:
            try:
                initial_revert_then_restore(config.start_block)

                shared.set_initial_start_block_if_absent(config.start_block)
                shared.set_current_start_block(config.start_block)

                logger.info(
                    "TransactionSenderRunner starting: feeder_url=%s sequencer_url=%s start_block=%s end_block=%s "
                    "timeout=%s retries=%s backoff=%s sleep=%s",
                    config.feeder_url,
                    config.sequencer_url,
                    config.start_block,
                    config.end_block,
                    config.request_timeout_seconds,
                    config.retries,
                    config.retry_backoff_seconds,
                    config.sleep_between_blocks_seconds,
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


def start_background_sender(
    *,
    feeder_url: str = consts.CONFIG.feeder.base_url,
    sequencer_url: str = consts.CONFIG.sequencer.base_url_default,
    start_block: int = consts.CONFIG.blocks.start_block,
    end_block: int = consts.CONFIG.blocks.end_block,
    request_timeout_seconds: float = 15.0,
    retries: int = 3,
    retry_backoff_seconds: float = 0.5,
    sleep_between_blocks_seconds: float = consts.CONFIG.sleep.sleep_between_blocks_seconds,
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
        sleep_between_blocks_seconds=sleep_between_blocks_seconds,
    )
    return TransactionSenderRunner.background().start(config=cfg)
