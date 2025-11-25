import asyncio
from typing import Any, Dict


async def handle_l1_handler_tx(tx: Dict[str, Any]) -> None:
    # Minimal async sink: just print the tx hash for now
    txh = tx.get("transaction_hash")
    print(f"[L1_HANDLER] {txh}")
    # Tiny async yield to keep cooperative scheduling
    await asyncio.sleep(0)
