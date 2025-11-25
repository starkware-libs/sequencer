import asyncio
from typing import Any, Dict, List, Optional, Set

import aiohttp
import logging
import threading
from consts import (
    ADD_TX_ENDPOINT,
    BLOCKED_SENDERS,
    END_BLOCK_DEFAULT,
    FEEDER_BASE_URL,
    SEQUENCER_BASE_URL_DEFAULT,
    SLEEP_BETWEEN_BLOCKS_SECONDS_DEFAULT,
    START_BLOCK_DEFAULT,
)
from feeder_client import FeederClient
from l1_handler import handle_l1_handler_tx
from shared_context import shared

logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s")


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
            logging.warning(
                f"Fetch block {block_number} failed on attempt {attempt + 1}/{retries + 1}: {err}. Retrying in {sleep_seconds:.2f}s"
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
        if response.status != 200:
            logging.warning(f"Forward failed ({response.status}): {text}")
            try:
                txh = tx.get("transaction_hash")
                if isinstance(txh, str) and txh:
                    shared.record_gateway_error(txh, response.status, text)
            except Exception:
                pass
        else:
            logging.info(f"Forwarded tx: {tx.get('transaction_hash', 'N/A')}")
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


async def stream_blocks(
    feeder_url: str,
    sequencer_url: str,
    start_block: int,
    *,
    request_timeout_seconds: float,
    retries: int,
    retry_backoff_seconds: float,
    sleep_between_blocks_seconds: float,
    end_block: Optional[int],
    stop_event: asyncio.Event,
) -> None:
    """Stream blocks from feeder, compute stats, and optionally forward valid txs.

    The loop ends on SIGINT/SIGTERM (stop_event set) or when end_block is reached.
    """
    timeout = aiohttp.ClientTimeout(total=request_timeout_seconds)

    async with aiohttp.ClientSession(timeout=timeout) as session:
        feeder = FeederClient(base_url=feeder_url)
        block_number = start_block

        async def _forward_one(tx: Dict[str, Any], *, src_bn: int) -> None:
            if tx.get("type") == "L1_HANDLER":
                await handle_l1_handler_tx(tx)
            else:
                await send_transaction_to_http_server(
                    session, sequencer_url, tx, source_block_number=src_bn
                )

        def _is_deploy_account(tx: Dict[str, Any]) -> bool:
            return tx.get("type") == "DEPLOY_ACCOUNT"

        while not stop_event.is_set():
            if end_block is not None and block_number > end_block:
                break
            try:
                block = await fetch_block_transactions(
                    feeder,
                    block_number,
                    retries=retries,
                    retry_backoff_seconds=retry_backoff_seconds,
                )
                # Save the raw feeder (FGW) block in shared memory for later use by echo_center
                try:
                    shared.store_fgw_block(block_number, block)
                except Exception:
                    pass

                # transactions
                all_txs: List[Dict[str, Any]] = block.get("transactions", []) or []
                current_blocked: Set[str] = {s.lower() for s in BLOCKED_SENDERS if s}
                valid_txs = filter_valid_transactions(all_txs, current_blocked)

                logging.info(f"Block {block_number}: total={len(all_txs)}, valid={len(valid_txs)})")

                # Update in-memory revert errors mapped to tx hashes for this block
                mappings = _extract_receipt_revert_error_mappings(block)
                if mappings:
                    for m in mappings:
                        for h, err in m.items():
                            shared.add_mainnet_revert_error(h, err)

                # Forward valid txs: DEPLOY_ACCOUNT first, then the rest
                if valid_txs:
                    deploy_txs = [tx for tx in valid_txs if _is_deploy_account(tx)]
                    other_txs = [tx for tx in valid_txs if not _is_deploy_account(tx)]

                    if deploy_txs:
                        for tx in deploy_txs:
                            await _forward_one(tx, src_bn=block_number)
                    if other_txs:
                        for tx in other_txs:
                            await _forward_one(tx, src_bn=block_number)

            except Exception as e:  # noqa: BLE001 - top-level loop protection
                logging.error(f"Error processing block {block_number}: {e}")

            block_number += 1
            if sleep_between_blocks_seconds > 0:
                try:
                    await asyncio.wait_for(stop_event.wait(), timeout=sleep_between_blocks_seconds)
                except asyncio.TimeoutError:
                    pass


class TransactionSenderRunner:
    """Run stream_blocks in a dedicated asyncio event loop thread with start/stop controls."""

    def __init__(self) -> None:
        self._thread: Optional[threading.Thread] = None
        self._loop: Optional[asyncio.AbstractEventLoop] = None
        self._stop_event: Optional[asyncio.Event] = None
        self._running: bool = False
        self._lock = threading.Lock()

    def is_running(self) -> bool:
        with self._lock:
            return self._running

    def start(
        self,
        *,
        feeder_url: str = FEEDER_BASE_URL,
        sequencer_url: str = SEQUENCER_BASE_URL_DEFAULT,
        start_block: int = START_BLOCK_DEFAULT,
        end_block: Optional[int] = END_BLOCK_DEFAULT,
        request_timeout_seconds: float = 15.0,
        retries: int = 3,
        retry_backoff_seconds: float = 0.5,
        sleep_between_blocks_seconds: float = SLEEP_BETWEEN_BLOCKS_SECONDS_DEFAULT,
    ) -> bool:
        with self._lock:
            if self._running:
                return False
            self._running = True

        def _runner() -> None:
            try:
                self._loop = asyncio.new_event_loop()
                asyncio.set_event_loop(self._loop)
                self._stop_event = asyncio.Event()
                logging.getLogger().setLevel(logging.INFO)

                logging.info(
                    "TransactionSenderRunner starting: feeder_url=%s sequencer_url=%s start_block=%s end_block=%s "
                    "timeout=%s retries=%s backoff=%s sleep=%s",
                    feeder_url,
                    sequencer_url,
                    start_block,
                    end_block,
                    request_timeout_seconds,
                    retries,
                    retry_backoff_seconds,
                    sleep_between_blocks_seconds,
                )

                async def _main() -> None:
                    await stream_blocks(
                        feeder_url=feeder_url,
                        sequencer_url=sequencer_url,
                        start_block=start_block,
                        request_timeout_seconds=request_timeout_seconds,
                        retries=retries,
                        retry_backoff_seconds=retry_backoff_seconds,
                        sleep_between_blocks_seconds=sleep_between_blocks_seconds,
                        end_block=end_block,
                        stop_event=self._stop_event
                        if self._stop_event is not None
                        else asyncio.Event(),
                    )

                self._loop.run_until_complete(_main())
            finally:
                logging.info("TransactionSenderRunner stopped")
                with self._lock:
                    self._running = False
                try:
                    if self._loop is not None:
                        self._loop.stop()
                        self._loop.close()
                except Exception:
                    pass

        self._thread = threading.Thread(target=_runner, name="TransactionSenderRunner", daemon=True)
        self._thread.start()
        return True

    def stop(self, *, join_timeout_seconds: float = 0.0) -> bool:
        with self._lock:
            if not self._running:
                return False
            loop = self._loop
            stop_event = self._stop_event
            th = self._thread

        if loop is not None and stop_event is not None:
            try:
                loop.call_soon_threadsafe(stop_event.set)
            except Exception:
                pass
        if th is not None and join_timeout_seconds > 0:
            try:
                th.join(timeout=join_timeout_seconds)
            except Exception:
                pass
        return True


_RUNNER_SINGLETON: Optional[TransactionSenderRunner] = None


def _get_runner() -> TransactionSenderRunner:
    global _RUNNER_SINGLETON
    if _RUNNER_SINGLETON is None:
        _RUNNER_SINGLETON = TransactionSenderRunner()
    return _RUNNER_SINGLETON


def start_background_sender(
    *,
    feeder_url: str = FEEDER_BASE_URL,
    sequencer_url: str = SEQUENCER_BASE_URL_DEFAULT,
    start_block: int = START_BLOCK_DEFAULT,
    end_block: Optional[int] = END_BLOCK_DEFAULT,
    request_timeout_seconds: float = 15.0,
    retries: int = 3,
    retry_backoff_seconds: float = 0.5,
    sleep_between_blocks_seconds: float = SLEEP_BETWEEN_BLOCKS_SECONDS_DEFAULT,
) -> bool:
    runner = _get_runner()
    return runner.start(
        feeder_url=feeder_url,
        sequencer_url=sequencer_url,
        start_block=start_block,
        end_block=end_block,
        request_timeout_seconds=request_timeout_seconds,
        retries=retries,
        retry_backoff_seconds=retry_backoff_seconds,
        sleep_between_blocks_seconds=sleep_between_blocks_seconds,
    )


def stop_background_sender(*, join_timeout_seconds: float = 0.0) -> bool:
    runner = _get_runner()
    return runner.stop(join_timeout_seconds=join_timeout_seconds)


def is_sender_running() -> bool:
    return _get_runner().is_running()
