import asyncio
import json
import os
from datetime import datetime
from typing import Any, Dict, List, Optional, Set

import aiohttp
import base64
import consts
import contextlib
import gzip
import io
import reports
import threading
from consts import (
    ADD_TX_ENDPOINT,
    BLOCKED_SENDERS,
    END_BLOCK_DEFAULT,
    FEEDER_BASE_URL,
    INITIAL_SLOW_BLOCKS_COUNT,
    SEQUENCER_BASE_URL_DEFAULT,
    SLEEP_BETWEEN_BLOCKS_SECONDS_DEFAULT,
)
from feeder_client import FeederClient
from logger import get_logger
from manage_sequencer import initial_revert_then_restore, resync_sequencer, scale_sequencer_to_zero
from shared_context import l1_manager, shared

logger = get_logger("transaction_sender")


def _extract_receipt_revert_error_mappings(block: Dict[str, Any]) -> List[Dict[str, str]]:
    """
    Pair transaction_receipts[i].revert_error with transactions[i].transaction_hash.
    Returns a list of {hash: revert_error} for entries where revert_error is present.
    """
    out: List[Dict[str, str]] = []
    for idx, receipt in enumerate(block["transaction_receipts"]):
        revert_error = receipt.get("revert_error")
        if not revert_error:
            continue
        out.append({block["transactions"][idx]["transaction_hash"]: revert_error})
    return out


def _compress_and_encode_json(value: Any) -> str:
    """
    Mirror the Rust `compress_and_encode` helper:
    - JSON-serialize `value`
    - gzip-compress the JSON bytes
    - base64-encode the compressed payload
    """
    # Standard JSON encoding; whitespace differences are fine for the Rust decoder.
    json_bytes = json.dumps(value).encode("utf-8")
    compressed = gzip.compress(json_bytes)
    return base64.b64encode(compressed).decode("ascii")


async def fetch_block_transactions(
    feeder: FeederClient,
    block_number: int,
    *,
    retries: int = 3,
    retry_backoff_seconds: float = 0.5,
) -> Dict[str, Any]:
    """Fetch a block from the feeder with simple retries.
    Raises an exception after exhausting retries.
    """
    attempt = 0
    last_err: Optional[Exception] = None

    while attempt <= retries:
        try:
            return feeder.get_block(block_number)
        except Exception as err:  # noqa: BLE001 - top-level retry handler
            last_err = err
            if attempt == retries:
                break
            sleep_seconds = retry_backoff_seconds * (2**attempt)
            logger.warning(
                f"Fetch block {block_number} failed on attempt {attempt + 1}/{retries + 1}: {err}. "
                f"Retrying in {sleep_seconds:.2f}s"
            )
            await asyncio.sleep(sleep_seconds)
            attempt += 1

    assert last_err is not None
    raise last_err


def filter_valid_transactions(
    all_transactions: List[Dict[str, Any]],
    blocked_senders: Set[str],
) -> List[Dict[str, Any]]:
    """Return only transactions that match forwarding criteria."""
    return [
        tx for tx in all_transactions if tx.get("sender_address", "").lower() not in blocked_senders
    ]


async def send_transaction_to_http_server(
    session: aiohttp.ClientSession,
    sequencer_url: str,
    tx: Dict[str, Any],
    *,
    source_block_number: int,
) -> None:
    url = f"{sequencer_url}{ADD_TX_ENDPOINT}"
    headers = {"Content-Type": "application/json"}
    async with session.post(url, json=tx, headers=headers) as response:
        text = await response.text()
        if response.status != consts.HTTP_OK:
            logger.warning(f"Forward failed ({response.status}): {text}")
            try:
                txh = tx.get("transaction_hash")
                if isinstance(txh, str) and txh:
                    shared.record_gateway_error(
                        txh, response.status, text, block_number=int(source_block_number)
                    )
            except Exception:
                pass
        else:
            logger.info(f"Forwarded tx: {tx.get('transaction_hash', 'N/A')}")
            # Track the sent transaction in shared memory
            try:
                txh = tx.get("transaction_hash")
                if isinstance(txh, str) and txh:
                    shared.record_sent_tx(txh, int(source_block_number))
            except Exception:
                pass

    # If the sent transaction was a DEPLOY_ACCOUNT, sleep briefly
    tx_type_value = tx.get("type")
    if tx_type_value == "DEPLOY_ACCOUNT":
        await asyncio.sleep(2)


class TransactionSenderService:
    """Encapsulate the streaming and forwarding logic in a class-based API."""

    async def stream_blocks(
        self,
        feeder_url: str,
        sequencer_url: str,
        start_block: int,
        *,
        request_timeout_seconds: float,
        retries: int,
        retry_backoff_seconds: float,
        sleep_between_blocks_seconds: float,
        end_block: Optional[int],
    ) -> None:
        """Stream blocks from feeder, compute stats, and optionally forward valid txs.
        The loop ends on SIGINT/SIGTERM (stop_event set) or when end_block is reached.
        """
        timeout = aiohttp.ClientTimeout(total=request_timeout_seconds)

        async with aiohttp.ClientSession(timeout=timeout) as session:
            feeder = FeederClient(base_url=feeder_url)
            # Bounded queue shared between producer (block reader) and consumer (tx sender).
            # The producer will backpressure when the queue reaches maxsize.
            tx_queue: "asyncio.Queue[Dict[str, Any]]" = asyncio.Queue(maxsize=100)
            SENTINEL: Dict[str, Any] = {"_sentinel": True}

            async def _forward_one(
                tx: Dict[str, Any], *, src_bn: int, src_ts: Optional[int] = None
            ) -> None:
                """Forward or handle a single transaction."""
                if tx.get("type") == "L1_HANDLER":
                    # Use shared L1Manager to update any L1-related cache/state based on this tx.
                    try:
                        logger.info(f"Setting new tx: {tx}, src_bn: {src_bn}, src_ts: {src_ts}")
                        l1_manager.set_new_tx(tx, src_ts)
                        txh = tx["transaction_hash"]
                        shared.record_sent_tx(txh, src_bn)
                    except Exception as err:
                        logger.warning(
                            f"Failed to update L1 cache for tx {tx.get('transaction_hash')}: {err}"
                        )
                    return

                out_tx = tx
                if tx.get("type") == "DECLARE":
                    class_hash_val = tx.get("class_hash")
                    cc_obj = feeder.get_class_by_hash(class_hash_val)
                    sierra_program_val = cc_obj.get("sierra_program")
                    sierra_program_encoded = _compress_and_encode_json(sierra_program_val)
                    cc_obj = {**cc_obj, "sierra_program": sierra_program_encoded}
                    abi_val = cc_obj.get("abi")
                    cc_obj = {**cc_obj, "abi": json.dumps(abi_val)}
                    out_tx = {**tx, "contract_class": cc_obj}

                await send_transaction_to_http_server(
                    session, sequencer_url, out_tx, source_block_number=src_bn
                )

            def _is_deploy_account(tx: Dict[str, Any]) -> bool:
                return tx.get("type") == "DEPLOY_ACCOUNT"

            async def producer() -> None:
                """Fetch blocks from the feeder and enqueue their valid transactions."""
                nonlocal start_block
                block_number = start_block

                async def _clear_tx_queue() -> None:
                    """Remove all pending items from the tx queue (used on resync)."""
                    while True:
                        try:
                            _ = tx_queue.get_nowait()
                            tx_queue.task_done()
                        except asyncio.QueueEmpty:
                            break

                while True:
                    if end_block is not None and block_number > end_block:
                        break
                    shared.set_sender_current_block(block_number)
                    try:
                        block = await fetch_block_transactions(
                            feeder,
                            block_number,
                            retries=retries,
                            retry_backoff_seconds=retry_backoff_seconds,
                        )
                        # Record block timestamps for first/latest diffs
                        ts_val = int(block.get("timestamp"))
                        shared.set_first_block_timestamp_if_absent(ts_val)
                        shared.set_latest_block_timestamp(ts_val)
                        # Save the raw FGW block in shared memory for later use by echo_center
                        shared.store_fgw_block(block_number, block)

                        # transactions
                        all_txs: List[Dict[str, Any]] = block.get("transactions", []) or []
                        current_blocked: Set[str] = {s.lower() for s in BLOCKED_SENDERS if s}
                        valid_txs = filter_valid_transactions(all_txs, current_blocked)

                        logger.info(
                            f"Block {block_number}: total={len(all_txs)}, valid={len(valid_txs)})"
                        )

                        # Update in-memory revert errors mapped to tx hashes for this block
                        mappings = _extract_receipt_revert_error_mappings(block)
                        if mappings:
                            for m in mappings:
                                for h, err in m.items():
                                    shared.add_mainnet_revert_error(h, err)

                        # Enqueue valid txs: DEPLOY_ACCOUNT first, then the rest.
                        if valid_txs:
                            deploy_txs = [tx for tx in valid_txs if _is_deploy_account(tx)]
                            other_txs = [tx for tx in valid_txs if not _is_deploy_account(tx)]

                            # The queue is bounded, so these puts will backpressure when full.
                            for tx in deploy_txs:
                                await tx_queue.put(
                                    {"tx": tx, "src_bn": block_number, "src_ts": None}
                                )
                            for tx in other_txs:
                                await tx_queue.put(
                                    {"tx": tx, "src_bn": block_number, "src_ts": ts_val}
                                )
                            shared.record_forwarded_block(block_number, len(valid_txs))

                    except Exception as e:  # noqa: BLE001 - top-level loop protection
                        logger.error(f"Error processing block {block_number}: {e}")
                        # If the error indicates the sequencer is unreachable, wait briefly and
                        # retry the same block. This covers feeder/http issues encountered while
                        # fetching or preparing the block.
                        msg = str(e)
                        is_connect_error = (
                            isinstance(e, aiohttp.ClientConnectorError)
                            or "Cannot connect to host" in msg
                            or "Connect call failed" in msg
                        )
                        if is_connect_error:
                            wait_seconds = max(retry_backoff_seconds, 1.0)
                            logger.warning(
                                "Sequencer/feeder connection error while processing block "
                                f"{block_number}; will retry this block after {wait_seconds:.1f}s"
                            )
                            await asyncio.sleep(wait_seconds)
                            # Do not run resync logic or advance block_number; go back to the top
                            # of the loop and retry this same block.
                            continue

                    snapshot = shared.get_report_snapshot()

                    trigger_tx_hash: Optional[str] = None
                    trigger_block: Optional[int] = None
                    trigger_reason: Optional[str] = None

                    gw = snapshot.get("gateway_errors") or {}
                    sent_map: Dict[str, int] = snapshot.get("sent_tx_hashes") or {}

                    # Only consider errors / pending txs that are at least 10 blocks old
                    threshold_block = int(block_number) - 20

                    # Collect all eligible errors (gateway + not-committed) with reasons
                    candidates: List[tuple[str, int, str]] = []

                    if gw:
                        for txh, info in gw.items():
                            bn = int(info.get("block_number"))
                            if bn <= threshold_block:
                                resp = info.get("response")
                                reason = f"Gateway error: {resp}"
                                candidates.append((str(txh), bn, reason))

                    if sent_map:
                        for txh, bn in sent_map.items():
                            bn_int = int(bn)
                            if bn_int <= threshold_block:
                                reason = "Still pending after >=10 blocks"
                                candidates.append((str(txh), bn_int, reason))

                    # Trigger resync only once the combined number of errors meets the threshold.
                    # When that happens, resync from the first (oldest) error block.
                    if len(candidates) >= consts.RESYNC_ERROR_THRESHOLD:
                        txh_first, bn_first, reason_first = min(
                            candidates, key=lambda item: item[1]
                        )
                        trigger_tx_hash = txh_first
                        trigger_block = bn_first
                        trigger_reason = (
                            f"Resync after >= {consts.RESYNC_ERROR_THRESHOLD} errors: "
                            f"{reason_first}"
                        )

                    if trigger_tx_hash is not None and trigger_block is not None:
                        logger.warning(
                            "Resync triggered by tx "
                            f"{trigger_tx_hash} at block {trigger_block}: {trigger_reason}"
                        )
                        # Drop any queued-but-unsent transactions; we'll restart clean
                        # from next_start_block below.
                        await _clear_tx_queue()
                        # Record cause; if repeated, treat as certain failure and skip to block+1
                        is_repeat = shared.record_resync_cause(
                            trigger_tx_hash, trigger_block, trigger_reason or ""
                        )

                        next_start_block = trigger_block + 1 if is_repeat else trigger_block

                        # Scale down sequencer pod to 0 replicas (wait) before clearing and resync
                        scale_sequencer_to_zero()

                        # Clear in-memory tracking before resync
                        try:
                            # Write reports to separate timestamped files just before clearing state
                            try:
                                # payload constructed from in-memory snapshot
                                buf_snapshot = io.StringIO()
                                buf_reverts = io.StringIO()
                                # Build snapshot in-memory (avoid HTTP dependency)
                                base_snapshot = shared.get_report_snapshot()
                                payload = {
                                    **base_snapshot,
                                    "sent_empty": len(base_snapshot.get("sent_tx_hashes") or {})
                                    == 0,
                                }
                                # Prepare header/meta (included in both files for self-containment)
                                header_lines = []
                                header_lines.append("===== Echonet reports before resync =====")
                                header_lines.append(f"timestamp: {datetime.utcnow().isoformat()}Z")
                                header_lines.append(f"trigger_tx_hash: {trigger_tx_hash}")
                                header_lines.append(f"trigger_block: {trigger_block}")
                                header_lines.append(f"trigger_reason: {trigger_reason}")
                                header = "\n".join(header_lines) + "\n\n"
                                # Snapshot (main report)
                                with contextlib.redirect_stdout(buf_snapshot):
                                    print(header, end="")
                                    reports._print_snapshot_from_data(
                                        payload
                                    )  # type: ignore[attr-defined]
                                # Reverts comparison (include details in file)
                                with contextlib.redirect_stdout(buf_reverts):
                                    print(header, end="")
                                    reports._compare_reverts_from_data(
                                        payload
                                    )  # type: ignore[attr-defined]

                                # Also print the reports to the application logs before resync.
                                snapshot_str = buf_snapshot.getvalue()
                                logger.info(
                                    "Echonet report snapshot before resync:\n%s", snapshot_str
                                )
                                os.makedirs(str(consts.LOG_DIR), exist_ok=True)
                                ts_suffix = datetime.utcnow().strftime("%Y%m%dT%H%M%SZ")
                                out_snapshot = os.path.join(
                                    str(consts.LOG_DIR),
                                    f"report_snapshot_{ts_suffix}.log",
                                )
                                out_reverts = os.path.join(
                                    str(consts.LOG_DIR),
                                    f"report_reverts_{ts_suffix}.log",
                                )
                                with open(out_snapshot, "w", encoding="utf-8") as f1:
                                    f1.write(buf_snapshot.getvalue())
                                    f1.write("\n===== End report =====\n")
                                with open(out_reverts, "w", encoding="utf-8") as f2:
                                    f2.write(buf_reverts.getvalue())
                                    f2.write("\n===== End report =====\n")
                            except Exception as rep_err:
                                logger.warning(f"Failed to write reports before resync: {rep_err}")
                            shared.clear_for_resync()
                        except Exception:
                            pass

                        # Execute resync (blocking) in a thread
                        try:
                            loop = asyncio.get_running_loop()
                            await loop.run_in_executor(None, resync_sequencer, next_start_block)
                        except Exception as err:
                            logger.error(f"Failed to resync sequencer: {err}")
                        try:
                            consts.START_BLOCK_DEFAULT = int(next_start_block)
                        except Exception:
                            pass
                        # Track the new effective start block in shared context
                        shared.set_current_start_block(int(next_start_block))
                        # Reset counters and set new block number (start "from beginning" at the new point)
                        block_number = int(next_start_block)
                    else:
                        # Normal progression to next block if no resync
                        block_number += 1

                    # Dynamic sleep: add +3s during the first X blocks from starting block
                    effective_sleep = sleep_between_blocks_seconds
                    start_bn = int(consts.START_BLOCK_DEFAULT)
                    if block_number is not None and (int(block_number) - start_bn) < int(
                        INITIAL_SLOW_BLOCKS_COUNT
                    ):
                        effective_sleep = (
                            float(sleep_between_blocks_seconds) + consts.EXTRA_SLEEP_TIME_SECONDS
                        )

                    if effective_sleep > 0:
                        await asyncio.sleep(effective_sleep)

            async def consumer() -> None:
                """Continuously read transactions from the queue and send them to the node."""
                while True:
                    item = await tx_queue.get()
                    try:
                        if item is SENTINEL or item.get("_sentinel"):
                            # Sentinel received – stop consumer.
                            return

                        tx = item["tx"]
                        src_bn = int(item["src_bn"])
                        src_ts = item.get("src_ts")

                        # Retry on connection errors for this tx; other errors are just logged.
                        while True:
                            try:
                                await _forward_one(tx, src_bn=src_bn, src_ts=src_ts)
                                break
                            except Exception as e:  # noqa: BLE001
                                msg = str(e)
                                is_connect_error = (
                                    isinstance(e, aiohttp.ClientConnectorError)
                                    or "Cannot connect to host" in msg
                                    or "Connect call failed" in msg
                                )
                                if is_connect_error:
                                    wait_seconds = max(retry_backoff_seconds, 1.0)
                                    logger.warning(
                                        "Sequencer connection error while sending tx "
                                        f"{tx.get('transaction_hash')}; "
                                        f"retrying after {wait_seconds:.1f}s"
                                    )
                                    await asyncio.sleep(wait_seconds)
                                    # Retry the same transaction.
                                    continue

                                logger.error(
                                    "Error forwarding tx "
                                    f"{tx.get('transaction_hash', 'N/A')}: {e}"
                                )
                                break
                    finally:
                        tx_queue.task_done()

            # Run producer and consumer concurrently.
            producer_task = asyncio.create_task(producer())
            consumer_task = asyncio.create_task(consumer())

            # Wait for producer to finish (end_block reached or external stop).
            await producer_task

            # Signal consumer to exit once it has drained the queue.
            await tx_queue.put(SENTINEL)
            await tx_queue.join()
            await consumer_task


class TransactionSenderRunner:
    """Run stream_blocks in a dedicated asyncio event loop thread with start/stop controls."""

    def __init__(self) -> None:
        self._thread: Optional[threading.Thread] = None
        self._service = TransactionSenderService()

    def start(
        self,
        *,
        feeder_url: str = FEEDER_BASE_URL,
        sequencer_url: str = SEQUENCER_BASE_URL_DEFAULT,
        start_block: Optional[int] = None,
        end_block: Optional[int] = END_BLOCK_DEFAULT,
        request_timeout_seconds: float = 15.0,
        retries: int = 3,
        retry_backoff_seconds: float = 0.5,
        sleep_between_blocks_seconds: float = SLEEP_BETWEEN_BLOCKS_SECONDS_DEFAULT,
    ) -> bool:
        # If a thread is already running, do not start another one.
        if self._thread is not None and self._thread.is_alive():
            return False

        def _runner() -> None:
            try:
                # Perform initial revert to align sequencer with starting block
                initial_revert_then_restore(int(consts.START_BLOCK_DEFAULT))

                # Initialize shared-context start blocks for reporting
                effective_start = (
                    start_block if start_block is not None else consts.START_BLOCK_DEFAULT
                )
                shared.set_initial_start_block_if_absent(int(effective_start))
                shared.set_current_start_block(int(effective_start))

                logger.info(
                    "TransactionSenderRunner starting: "
                    f"feeder_url={feeder_url} "
                    f"sequencer_url={sequencer_url} "
                    f"start_block={start_block if start_block is not None else consts.START_BLOCK_DEFAULT} "
                    f"end_block={end_block} "
                    f"timeout={request_timeout_seconds} "
                    f"retries={retries} "
                    f"backoff={retry_backoff_seconds} "
                    f"sleep={sleep_between_blocks_seconds}"
                )

                asyncio.run(
                    self._service.stream_blocks(
                        feeder_url=feeder_url,
                        sequencer_url=sequencer_url,
                        start_block=(
                            start_block if start_block is not None else consts.START_BLOCK_DEFAULT
                        ),
                        request_timeout_seconds=request_timeout_seconds,
                        retries=retries,
                        retry_backoff_seconds=retry_backoff_seconds,
                        sleep_between_blocks_seconds=sleep_between_blocks_seconds,
                        end_block=end_block,
                    )
                )
            finally:
                logger.info("TransactionSenderRunner stopped")

        self._thread = threading.Thread(target=_runner, name="TransactionSenderRunner", daemon=True)
        self._thread.start()
        return True


def start_background_sender(
    *,
    feeder_url: str = FEEDER_BASE_URL,
    sequencer_url: str = SEQUENCER_BASE_URL_DEFAULT,
    start_block: Optional[int] = None,
    end_block: Optional[int] = END_BLOCK_DEFAULT,
    request_timeout_seconds: float = 15.0,
    retries: int = 3,
    retry_backoff_seconds: float = 0.5,
    sleep_between_blocks_seconds: float = SLEEP_BETWEEN_BLOCKS_SECONDS_DEFAULT,
) -> bool:
    """
    Start the background transaction sender.
    """
    runner = TransactionSenderRunner()
    return runner.start(
        feeder_url=feeder_url,
        sequencer_url=sequencer_url,
        start_block=start_block if start_block is not None else consts.START_BLOCK_DEFAULT,
        end_block=end_block,
        request_timeout_seconds=request_timeout_seconds,
        retries=retries,
        retry_backoff_seconds=retry_backoff_seconds,
        sleep_between_blocks_seconds=sleep_between_blocks_seconds,
    )
