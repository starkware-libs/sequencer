import argparse
import asyncio
import json
from pathlib import Path
from typing import Any, Dict, List, Optional

import aiohttp
import logging
import signal

logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s")

TX_LOG_FILE = str(Path(__file__).resolve().parent / "sent_txs.log")
ERRORS_FILE = str(Path(__file__).resolve().parent / "errors.jsonl")
# Mappings from transaction_hash -> revert_error extracted from feeder blocks
RECEIPT_ERRORS_FILE = str(Path(__file__).resolve().parent / "revert_errors.jsonl")

GET_BLOCK_ENDPOINT = "/feeder_gateway/get_block"
ADD_TX_ENDPOINT = "/gateway/add_transaction"

FEEDER_HEADERS = {"X-Throttling-Bypass": "QYHGVPY7PHER3QHI6LWBY25AGF5GGEZ"}

BLOCKED_SENDER_ADDRESSES = {}

def _append_lines(path: str, lines: List[str]) -> None:
    with open(path, "a", encoding="utf-8") as f:
        for line in lines:
            txt = str(line).rstrip("\n")
            f.write(txt + "\n")


def _extract_receipt_revert_error_mappings(block: Dict[str, Any]) -> List[Dict[str, str]]:
    """
    Pair transaction_receipts[i].revert_error with transactions[i].transaction_hash.
    Returns a list of {hash: revert_error} for entries where revert_error is present.
    """
    receipts = block.get("transaction_receipts", []) or []
    txs = block.get("transactions", []) or []
    out: List[Dict[str, str]] = []
    for idx, receipt in enumerate(receipts):
        if not isinstance(receipt, dict):
            continue
        err = receipt.get("revert_error")
        if not (isinstance(err, str) and err):
            continue
        tx = txs[idx] if idx < len(txs) else None
        tx_hash = tx.get("transaction_hash") if isinstance(tx, dict) else None
        if isinstance(tx_hash, str) and tx_hash:
            out.append({tx_hash: err})
    return out


def setup_receipt_errors_file(path: str) -> None:
    """Clear the receipt errors JSONL file at startup each run."""
    try:
        with open(path, "w", encoding="utf-8"):
            pass
    except Exception as e:
        logging.error(f"Failed to clear receipt errors file: {e}")


async def fetch_block_transactions(
    session: aiohttp.ClientSession,
    feeder_url: str,
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
    url = f"{feeder_url}{GET_BLOCK_ENDPOINT}?blockNumber={block_number}"

    while attempt <= retries:
        try:
            async with session.get(url, headers=FEEDER_HEADERS) as response:
                if response.status != 200:
                    text = await response.text()
                    raise RuntimeError(
                        f"Failed to fetch block {block_number}: {response.status} {text}"
                    )
                return await response.json()
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


def filter_valid_transactions(all_transactions: List[Dict[str, Any]]) -> List[Dict[str, Any]]:
    """Return only transactions that match forwarding criteria."""
    return [
        tx
        for tx in all_transactions
        if str(tx.get("sender_address", "")).lower() not in BLOCKED_SENDER_ADDRESSES
    ]


async def send_transaction_to_http_server(
    session: aiohttp.ClientSession,
    sequencer_url: str,
    tx: Dict[str, Any],
    tx_logger: Optional[logging.Logger] = None,
    error_logger: Optional[logging.Logger] = None,
) -> None:
    url = f"{sequencer_url}{ADD_TX_ENDPOINT}"
    headers = {"Content-Type": "application/json"}
    async with session.post(url, json=tx, headers=headers) as response:
        text = await response.text()
        if response.status != 200:
            logging.warning(f"Forward failed ({response.status}): {text}")
            # Persist 400 errors for inspection; ignore other statuses unless needed later
            if response.status == 400 and error_logger is not None:
                try:
                    error_logger.info(
                        json.dumps(
                            {
                                "status": response.status,
                                "response": text,
                                "transaction_hash": tx.get("transaction_hash", "N/A"),
                                "tx": tx,
                            },
                            ensure_ascii=False,
                        )
                    )
                except Exception:
                    # Best-effort; do not raise in forwarding path
                    pass
        else:
            logging.info(f"Forwarded tx: {tx.get('transaction_hash', 'N/A')}")
            # Mirror the same info to the tx log file
            if tx_logger is not None:
                tx_logger.info(f"Forwarded tx: {tx.get('transaction_hash', 'N/A')}")

    # If the sent transaction was a DEPLOY_ACCOUNT, sleep briefly
    tx_type_value = tx.get("type")
    # if tx_type_value == "DEPLOY_ACCOUNT":
    await asyncio.sleep(1)


async def stream_blocks(
    feeder_url: str,
    sequencer_url: str,
    start_block: int,
    *,
    forward_concurrency: int,
    request_timeout_seconds: float,
    retries: int,
    retry_backoff_seconds: float,
    sleep_between_blocks_seconds: float,
    end_block: Optional[int],
    tx_logger: Optional[logging.Logger],
    error_logger: Optional[logging.Logger],
    stop_event: asyncio.Event,
) -> None:
    """Stream blocks from feeder, compute stats, and optionally forward valid txs.

    The loop ends on SIGINT/SIGTERM (stop_event set) or when end_block is reached.
    """
    timeout = aiohttp.ClientTimeout(total=request_timeout_seconds)

    async with aiohttp.ClientSession(timeout=timeout) as session:
        block_number = start_block
        semaphore = asyncio.Semaphore(max(1, forward_concurrency))

        async def _forward_one(tx: Dict[str, Any]) -> None:
            async with semaphore:
                await send_transaction_to_http_server(
                    session, sequencer_url, tx, tx_logger, error_logger
                )

        def _is_deploy_account(tx: Dict[str, Any]) -> bool:
            tx_type_value = tx.get("type")
            return tx_type_value == "DEPLOY_ACCOUNT"

        while not stop_event.is_set():
            if end_block is not None and block_number > end_block:
                break
            try:
                block = await fetch_block_transactions(
                    session,
                    feeder_url,
                    block_number,
                    retries=retries,
                    retry_backoff_seconds=retry_backoff_seconds,
                )

                # transactions
                all_txs: List[Dict[str, Any]] = block.get("transactions", []) or []
                valid_txs = filter_valid_transactions(all_txs)

                logging.info(f"Block {block_number}: total={len(all_txs)}, valid={len(valid_txs)})")

                # Write receipt revert errors mapped to tx hashes for this block (JSONL)
                try:
                    mappings = _extract_receipt_revert_error_mappings(block)
                    if mappings:
                        _append_lines(
                            RECEIPT_ERRORS_FILE,
                            [json.dumps(m, ensure_ascii=False) for m in mappings],
                        )
                except Exception:
                    # Best-effort; do not interrupt streaming on file I/O issues
                    pass

                # Forward valid txs: DEPLOY_ACCOUNT first, then the rest
                if valid_txs:
                    deploy_txs = [tx for tx in valid_txs if _is_deploy_account(tx)]
                    other_txs = [tx for tx in valid_txs if not _is_deploy_account(tx)]

                    if deploy_txs:
                        await asyncio.gather(*[_forward_one(tx) for tx in deploy_txs])
                    if other_txs:
                        await asyncio.gather(*[_forward_one(tx) for tx in other_txs])

            except Exception as e:  # noqa: BLE001 - top-level loop protection
                logging.error(f"Error processing block {block_number}: {e}")

            block_number += 1
            if sleep_between_blocks_seconds > 0:
                try:
                    await asyncio.wait_for(stop_event.wait(), timeout=sleep_between_blocks_seconds)
                except asyncio.TimeoutError:
                    pass


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Stream blocks and optionally forward transactions"
    )
    parser.add_argument(
        "--feeder-url", default="https://feeder.alpha-mainnet.starknet.io", help="Feeder base URL"
    )
    parser.add_argument(
        "--sequencer-url", default="http://localhost:8080", help="Sequencer base URL for forwarding"
    )
    parser.add_argument("--start-block", type=int, default=3486720, help="Starting block number")
    parser.add_argument(
        "--end-block", type=int, default=None, help="Optional inclusive end block number"
    )
    parser.add_argument("--concurrency", type=int, default=1, help="Max concurrent forwards")
    parser.add_argument(
        "--timeout", type=float, default=15.0, help="HTTP request timeout (seconds)"
    )
    parser.add_argument(
        "--retries", type=int, default=3, help="Number of retries for fetching a block"
    )
    parser.add_argument(
        "--backoff", type=float, default=0.5, help="Initial retry backoff (seconds)"
    )
    parser.add_argument("--sleep", type=float, default=2.0, help="Sleep between blocks (seconds)")
    parser.add_argument("--verbose", action="store_true", help="Enable debug logging")
    return parser.parse_args()


def setup_tx_logger(path: str) -> Optional[logging.Logger]:
    """Create a dedicated file logger for forwarded transactions, returning it."""
    try:
        # Clear the file
        with open(path, "w", encoding="utf-8"):
            pass
        # Configure a file-backed logger for forwarded tx lines only
        tx_logger = logging.getLogger("txfile")
        tx_logger.setLevel(logging.INFO)
        file_handler = logging.FileHandler(path, mode="a", encoding="utf-8")
        file_handler.setFormatter(logging.Formatter("%(asctime)s %(message)s"))
        tx_logger.handlers.clear()
        tx_logger.addHandler(file_handler)
        # Prevent duplication in console via root logger
        tx_logger.propagate = False
        return tx_logger
    except Exception as e:  # noqa: BLE001 - setup protection
        logging.error(f"Failed to set up tx log file: {e}")
        return None


def setup_error_logger(path: str) -> Optional[logging.Logger]:
    """Create a dedicated file logger for HTTP 400 errors; clears file each run."""
    try:
        # Clear the file
        with open(path, "w", encoding="utf-8"):
            pass
        err_logger = logging.getLogger("txerrors")
        err_logger.setLevel(logging.INFO)
        file_handler = logging.FileHandler(path, mode="a", encoding="utf-8")
        # Write plain lines (already JSON strings), with timestamp for context
        file_handler.setFormatter(logging.Formatter("%(asctime)s %(message)s"))
        err_logger.handlers.clear()
        err_logger.addHandler(file_handler)
        err_logger.propagate = False
        return err_logger
    except Exception as e:  # noqa: BLE001 - setup protection
        logging.error(f"Failed to set up error log file: {e}")
        return None


def setup_signal_handlers(stop_event: asyncio.Event) -> None:
    """Install SIGINT/SIGTERM handlers to signal the event."""
    loop = asyncio.get_running_loop()
    try:
        loop.add_signal_handler(signal.SIGINT, stop_event.set)
        loop.add_signal_handler(signal.SIGTERM, stop_event.set)
    except NotImplementedError:
        # Some platforms (e.g., Windows) do not support signal handlers in asyncio
        pass


async def _amain() -> None:
    args = _parse_args()
    logging.getLogger().setLevel(logging.DEBUG if args.verbose else logging.INFO)

    # Prepare dedicated tx logger (clear file for each new run)
    tx_logger = setup_tx_logger(TX_LOG_FILE)
    # Prepare dedicated error logger for 400 responses (clear file for each new run)
    error_logger = setup_error_logger(ERRORS_FILE)
    # Clear the receipt revert errors mapping file for each run
    setup_receipt_errors_file(RECEIPT_ERRORS_FILE)

    stop_event = asyncio.Event()
    setup_signal_handlers(stop_event)

    await stream_blocks(
        feeder_url=args.feeder_url,
        sequencer_url=args.sequencer_url,
        start_block=args.start_block,
        forward_concurrency=args.concurrency,
        request_timeout_seconds=args.timeout,
        retries=args.retries,
        retry_backoff_seconds=args.backoff,
        sleep_between_blocks_seconds=args.sleep,
        end_block=args.end_block,
        tx_logger=tx_logger,
        error_logger=error_logger,
        stop_event=stop_event,
    )


if __name__ == "__main__":
    asyncio.run(_amain())
